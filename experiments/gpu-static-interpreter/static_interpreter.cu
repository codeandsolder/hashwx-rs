#include <cuda_runtime.h>
#include <algorithm>
#include <chrono>
#include <cinttypes>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <stdexcept>
#include <string>
#include <vector>

#define CK(e) do { cudaError_t ec=(e); if(ec!=cudaSuccess){fprintf(stderr,"%s:%d CUDA: %s\n", __FILE__,__LINE__,cudaGetErrorString(ec));std::exit(1);} } while(0)

constexpr int NPROG = 32;
constexpr int NOP = 8;
constexpr int NWORDS = 2049;
constexpr int SCRATCH_BYTES = NWORDS*8;
constexpr int WARP=32;
struct ProgramData {
    uint32_t ops[NPROG*NOP];
    uint32_t unconditional[NPROG];
    uint64_t key[2];
};
static_assert(sizeof(ProgramData)==1168);

__device__ __forceinline__ uint64_t rotl(uint64_t x, int n) {return (x<<n)|(x>>(64-n));}
__device__ __forceinline__ uint64_t rotr(uint64_t x, int n) {return (x>>n)|(x<<(64-n));}

__device__ __forceinline__ void sip(uint64_t (&v)[4]) {
    v[0]+=v[1]; v[2]+=v[3];
    v[1]=rotl(v[1],13); v[3]=rotl(v[3],16);
    v[1]^=v[0]; v[3]^=v[2];
    v[0]=rotl(v[0],32);
    v[2]+=v[1]; v[0]+=v[3];
    v[1]=rotl(v[1],17); v[3]=rotl(v[3],21);
    v[1]^=v[2]; v[3]^=v[0];
    v[2]=rotl(v[2],32);
}

__device__ __forceinline__ void init(uint64_t (&v)[4], uint64_t (&k)[2], uint64_t nonce, volatile uint64_t (*r)[WARP], int lane) {
    v[0]=0x736f6d6570736575ULL^k[0];
    v[1]=0x646f72616e646f6dULL^k[1];
    v[2]=0x6c7967656e657261ULL^k[0];
    v[3]=0x7465646279746573ULL^k[1];
    v[3]^=nonce;
    sip(v);
    v[0]^=nonce;
    v[2]^=0xbb;
    #pragma unroll
    for(int z=0;z<3;++z) sip(v);
    r[0][lane]=v[3]; r[1][lane]=v[2]; r[2][lane]=v[1]; r[3][lane]=v[0];
    v[0]^=k[0]; v[1]^=k[1]; v[2]^=k[0]; v[3]^=k[1];
    #pragma unroll
    for(int z=0;z<4;++z) sip(v);
    r[4][lane]=v[3]; r[5][lane]=v[2]; r[6][lane]=v[1]; r[7][lane]=v[0];
    r[8][lane]=((r[0][lane]^r[4][lane])&~7ULL)|3ULL;
}

__device__ __forceinline__ uint64_t load_unaligned(const uint64_t* s, uint64_t offset) {
    const uint32_t i=static_cast<uint32_t>(offset)&16383u;
    const uint64_t a=s[i>>3];
    const uint32_t shift=(i&7u)*8u;
    if(shift==0) return a;
    const uint64_t b=s[(i>>3)+1];
    return (a>>shift)|(b<<(64-shift));
}

__global__ void static_hashwx(const ProgramData* __restrict__ programs,
                              uint64_t* __restrict__ memory,
                              uint64_t* __restrict__ output,
                              int active_lanes, int hashes_per_seed, uint64_t start_nonce) {
    __shared__ uint16_t instructions[256][WARP];
    __shared__ uint32_t unconditional[WARP];
    __shared__ uint64_t r[9][WARP];

    const int lane=threadIdx.x;
    const bool active=lane<active_lanes;
    const int idx=blockIdx.x*active_lanes+lane;

    uint64_t k[2]={0,0};
    if(active) {
        const ProgramData* data=programs+idx;
        k[0]=data->key[0]; k[1]=data->key[1];
        #pragma unroll 1
        for(int i=0;i<256;++i) {
            const uint32_t raw=data->ops[i];
            // Every opcode fits in 4 bits; RMCG's src=8 is implicit.
            // 3-bit virtual registers and 6-bit immediates encode the entire IR.
            instructions[i][lane]=static_cast<uint16_t>(
                (raw&15u)
                | (((raw>>8)&7u)<<4u)
                | (((raw>>16)&7u)<<7u)
                | ((raw>>24)<<10u));
        }
        uint32_t mask=0;
        #pragma unroll
        for(int i=0;i<NPROG;++i) mask|=(data->unconditional[i]&1u)<<i;
        unconditional[lane]=mask;
    }
    __syncthreads();
    if(!active)return;
    uint64_t* s=memory+static_cast<size_t>(idx)*NWORDS;
    uint64_t digest=0;
    for(int nonce_index=0;nonce_index<hashes_per_seed;++nonce_index) {
        uint64_t v[4];
        init(v,k,start_nonce+nonce_index,r,lane);
        s[2048]=r[8][lane];
        uint32_t sp=16384;
        for(int phase=0;phase<2;++phase) {
            for(int rep=0;rep<4;++rep) {
                int budget=32;
                for(int program=0;program<NPROG;++program) {
                    int prog_base=program*NOP;
                    do {
                        if(phase==0) {
                            sp-=64;
                            #pragma unroll
                            for(int j=0;j<8;++j) s[sp/8+j]=r[7-j][lane];
                        }
                        #pragma unroll 1
                        for(int op_index=0;op_index<NOP;++op_index) {
                            const uint32_t ins=instructions[prog_base+op_index][lane];
                            const uint32_t op=ins&15u;
                            const uint32_t dst=(ins>>4)&7u;
                            const uint32_t src=(ins>>7)&7u;
                            const uint32_t imm=ins>>10;
                            uint64_t value=r[dst][lane];
                            if(op==3) {
                                r[dst][lane]=rotr(value*r[8][lane],imm);
                                continue;
                            }
                            uint64_t operand=r[src][lane];
                            if(phase==1) operand=load_unaligned(s,operand);
                            if(op==0) value=(value|imm)*operand;
                            else if(op==1) value=(value^imm)*operand;
                            else if(op==2) value=(value+imm)*operand;
                            else {
                                uint64_t shifted;
                                if(op<=6) shifted=rotr(value,imm);
                                else if(op<=9) shifted=static_cast<uint64_t>(static_cast<int64_t>(value)>>imm);
                                else shifted=value>>imm;
                                if(op==4 || op==7 || op==10) value=shifted^operand;
                                else if(op==5 || op==8 || op==11) value=shifted+operand;
                                else value=shifted-operand;
                            }
                            r[dst][lane]=value;
                        }
                        const uint32_t first=instructions[prog_base][lane];
                        const uint64_t branch_flag=r[(first>>4)&7u][lane];
                        if(budget==0 || (((unconditional[lane]>>program)&1u)==0 && (branch_flag&32u)))break;
                        --budget;
                    }while(true);
                }
            }
        }
        uint64_t lo[4],hi[4];
        #pragma unroll
        for(int i=0;i<4;++i){lo[i]=r[i][lane];hi[i]=r[i+4][lane];}
        sip(lo);sip(hi);
        const uint64_t hash=lo[3]^hi[3]^r[8][lane];
        digest^=hash;
    }
    output[idx]=digest;
}

static std::vector<uint8_t> read_file(const char* path) {
    std::ifstream f(path,std::ios::binary | std::ios::ate);
    if(!f)throw std::runtime_error(std::string("open ")+path);
    auto n=f.tellg();f.seekg(0);
    std::vector<uint8_t> data(static_cast<size_t>(n));
    if(!f.read(reinterpret_cast<char*>(data.data()),n))throw std::runtime_error("read failure");
    return data;
}

int main(int argc,char**argv) {
    if(argc<2){fprintf(stderr,"usage: static_interpreter DATA_FOLDER\n");return 2;}
    const std::string data=argv[1];
    auto data_p=read_file((data+"/programs.bin").c_str());
    auto data_gold=read_file((data+"/gold.bin").c_str());
    if(data_p.size()%sizeof(ProgramData)!=0 || data_p.size()/sizeof(ProgramData)*32!=data_gold.size()){
        fprintf(stderr,"bad fixture sizes %zu %zu\n",data_p.size(),data_gold.size());return 2;
    }
    const size_t max_programs=data_p.size()/sizeof(ProgramData);
    cudaDeviceProp prop;
    CK(cudaGetDeviceProperties(&prop,0));
    int l2=0;
    CK(cudaDeviceGetAttribute(&l2,cudaDevAttrL2CacheSize,0));
    printf("CUDA_GPU name=%s SMs=%d L2_KiB=%d\n",prop.name,prop.multiProcessorCount,l2/1024);
    ProgramData* gpu_p=nullptr;
    uint64_t *gpu_s=nullptr,*gpu_out=nullptr;
    CK(cudaMalloc(&gpu_p,data_p.size()));
    CK(cudaMalloc(&gpu_s,max_programs*sizeof(uint64_t)*NWORDS));
    CK(cudaMalloc(&gpu_out,max_programs*sizeof(uint64_t)));
    CK(cudaMemcpy(gpu_p,data_p.data(),data_p.size(),cudaMemcpyHostToDevice));
    // Differential test across different generated programs AND nonce values,
    // before doing any throughput measurements.
    for(int nonce=0;nonce<4;++nonce) {
        constexpr int check_blocks=30, check_lanes=6, n=check_blocks*check_lanes;
        static_hashwx<<<check_blocks,32>>>(gpu_p,gpu_s,gpu_out,check_lanes,1,nonce);
        CK(cudaGetLastError());
        CK(cudaDeviceSynchronize());
        std::vector<uint64_t> output(n);
        CK(cudaMemcpy(output.data(),gpu_out,n*8,cudaMemcpyDeviceToHost));
        for(int i=0;i<n;++i) {
            uint64_t expected=0;
            std::memcpy(&expected,&data_gold[i*32+nonce*8],8);
            if(output[i]!=expected) {
                fprintf(stderr,"GOLD_NONCE_MISMATCH seed=%d nonce=%d got=%016" PRIx64 " want=%016" PRIx64 "\n",
                    i,nonce,output[i],expected);
                return 4;
            }
        }
    }
    puts("GOLDEN_180_SEEDS_4_NONCES_ALL_MATCH");
    for(auto [blocks,lanes]: std::vector<std::pair<int,int>>{
        {30,4},{64,3},{90,2}
    }) {
        const int active=blocks*lanes;
        if(active>static_cast<int>(max_programs))continue;
        static_hashwx<<<blocks,32>>>(gpu_p,gpu_s,gpu_out,lanes,1,0);
        CK(cudaGetLastError());
        CK(cudaDeviceSynchronize());
        std::vector<uint64_t> output(active);
        CK(cudaMemcpy(output.data(),gpu_out,active*8,cudaMemcpyDeviceToHost));
        int bad=0;
        for(int i=0;i<active;++i) {
            uint64_t expected=0;
            std::memcpy(&expected,&data_gold[i*32],8);
            if(output[i]!=expected) {
                if(bad<4)printf("MISMATCH blocks=%d lanes=%d seed=%d got=%016" PRIx64 " want=%016" PRIx64 "\n",blocks,lanes,i,output[i],expected);
                ++bad;
            }
        }
        if(bad){fprintf(stderr,"VERIFY_FAILED %d/%d\n",bad,active);return 3;}
        cudaEvent_t ev_start,ev_end;
        CK(cudaEventCreate(&ev_start));CK(cudaEventCreate(&ev_end));
        const int n=463;
        // Timed work includes the kernel, not program generation or memory transfer.
        CK(cudaEventRecord(ev_start));
        static_hashwx<<<blocks,32>>>(gpu_p,gpu_s,gpu_out,lanes,n,0);
        CK(cudaEventRecord(ev_end));CK(cudaEventSynchronize(ev_end));CK(cudaGetLastError());
        float ms=0;
        CK(cudaEventElapsedTime(&ms,ev_start,ev_end));
        printf("STATIC_INTERPRETER blocks=%d lanes=%d hashes=%d scratch_KiB=%zu ms=%.4f MHs=%.6f verified=%d\n",
            blocks,lanes,active*n,active*SCRATCH_BYTES/1024,ms,
            (static_cast<double>(active)*n)/(ms*1000.),active);
        fflush(stdout);
        CK(cudaEventDestroy(ev_start));CK(cudaEventDestroy(ev_end));
    }
    CK(cudaFree(gpu_p));CK(cudaFree(gpu_s));CK(cudaFree(gpu_out));
}
