// End-to-end pinned-F1 512-bit PCIM model: host-memory rings -> real HashWX
// and HashX execution queues -> host memory. Backpressure and bus response
// channels are simulated, without an AWS FPGA or SDK.
module tb_pcim_dma;
    logic clk=0,rst=1,wr_en=0;
    logic [31:0] wr_addr=0,wr_data=0,rd_addr=0;
    wire [31:0] rd_value;
    wire [15:0] pcim_awid,pcim_arid;
    wire [63:0] pcim_awaddr,pcim_araddr;
    wire [7:0] pcim_awlen,pcim_arlen;
    wire [2:0] pcim_awsize,pcim_arsize;
    wire pcim_awvalid,pcim_arvalid;
    logic pcim_awready=0,pcim_arready=0;
    wire [511:0] pcim_wdata;
    wire [63:0] pcim_wstrb;
    wire pcim_wlast,pcim_wvalid;
    logic pcim_wready=0;
    logic [15:0] pcim_bid=0,pcim_rid=0;
    logic [1:0] pcim_bresp=0,pcim_rresp=0;
    logic pcim_bvalid=0,pcim_rvalid=0,pcim_rlast=0;
    logic [511:0] pcim_rdata=0;
    wire pcim_bready,pcim_rready;
    hash_f1_dual dut(.*);
    always #5 clk=~clk;
    localparam logic [63:0] DESC_BASE=64'h0000_1234_0000_0000;
    localparam logic [63:0] RES_BASE=64'h0000_5678_0000_0000;
    logic [511:0] descriptor_mem[0:15];
    logic [511:0] completion_mem[0:15];
    logic [31:0] wx_cfg[0:131],hx_cfg[0:1032];
    logic [63:0] wx_gold[0:2];
    logic [255:0] hx_gold[0:2];
    logic read_pending=0,write_pending=0;
    logic [63:0] pending_araddr=0,pending_awaddr=0;
    integer read_delay=0;
    integer clock_count=0,reads=0,writes=0;

    // Stagger all channels to exercise held VALID and accepted handshakes.
    always_comb begin
        pcim_arready = clock_count%5 != 0;
        pcim_awready = clock_count%7 != 0;
        pcim_wready  = clock_count%3 != 0;
    end
    always_ff @(posedge clk) begin
        if (rst) begin
            clock_count<=0;read_pending<=0;write_pending<=0;
            pcim_rvalid<=0;pcim_bvalid<=0;
            read_delay<=0;reads<=0;writes<=0;
        end else begin
            clock_count<=clock_count+1;
            if (pcim_arvalid && pcim_arready) begin
                if (pcim_arlen!==0 || pcim_arsize!==6 || pcim_araddr[5:0]!==0
                    || pcim_arid!==0 || pcim_araddr<DESC_BASE)
                    $fatal(1,"invalid PCIM 64-byte descriptor read");
                if (read_pending) $fatal(1,"multiple PCIM reads outstanding");
                pending_araddr<=pcim_araddr;
                read_pending<=1; read_delay<=2;
            end
            if (read_pending && !pcim_rvalid) begin
                if (read_delay>0) read_delay<=read_delay-1;
                else begin
                    pcim_rdata<=descriptor_mem[(pending_araddr-DESC_BASE)>>6];
                    pcim_rresp<=0;pcim_rid<=0;pcim_rlast<=1;
                    pcim_rvalid<=1;
                end
            end
            if (pcim_rvalid && pcim_rready) begin
                pcim_rvalid<=0;read_pending<=0;reads<=reads+1;
            end
            if (pcim_awvalid && pcim_awready) begin
                if (pcim_awlen!==0 || pcim_awsize!==6 || pcim_awaddr[5:0]!==0
                    || pcim_awid!==0 || pcim_awaddr<RES_BASE)
                    $fatal(1,"invalid PCIM 64-byte completion write");
                if (write_pending) $fatal(1,"multiple PCIM writes outstanding");
                write_pending<=1;
                pending_awaddr<=pcim_awaddr;
            end
            if (pcim_wvalid && pcim_wready) begin
                if (!write_pending || pcim_wstrb!==64'hffffffffffffffff
                    || pcim_wlast!==1) $fatal(1,"malformed PCIM write data");
                completion_mem[(pending_awaddr-RES_BASE)>>6]<=pcim_wdata;
                write_pending<=0;
                pcim_bvalid<=1;pcim_bresp<=0;pcim_bid<=0;
                writes<=writes+1;
            end
            if(pcim_bvalid && pcim_bready) pcim_bvalid<=0;
        end
    end

    task automatic poke(input logic [31:0] addr,val);
        @(negedge clk);wr_en=1;wr_addr=addr;wr_data=val;
        @(negedge clk);wr_en=0;
    endtask
    task automatic peek(input logic [31:0] addr,val);
        rd_addr=addr;#1;
        if (rd_value!==val)
            $fatal(1,"register %x expected %x got %x",addr,val,rd_value);
    endtask
    task automatic make_desc(input integer slot,
        input logic [31:0] tag,algo,count,
        input logic [63:0] nonce,target);
        descriptor_mem[slot]='0;
        descriptor_mem[slot][31:0]=tag;
        descriptor_mem[slot][63:32]=algo;
        descriptor_mem[slot][95:64]=count;
        descriptor_mem[slot][191:128]=nonce;
        descriptor_mem[slot][255:192]=target;
    endtask
    initial begin
        $readmemh("tests/fixtures/config.hex",wx_cfg);
        $readmemh("tests/fixtures/golden.hex",wx_gold);
        $readmemh("tests/fixtures/hashx_config_00.hex",hx_cfg);
        $readmemh("tests/fixtures/hashx_golden_00.hex",hx_gold);
        for(integer k=0;k<16;k++) begin
            descriptor_mem[k]='0;completion_mem[k]='0;
        end
        repeat(3) @(negedge clk);rst=0;
        for(integer i=0;i<132;i++) poke(32'h800+32'(i*4),wx_cfg[i]);
        for(integer i=0;i<1033;i++) poke(32'h4000+32'(i*4),hx_cfg[i]);

        make_desc(0,10,0,2,0,0);
        make_desc(1,20,1,2,0,0);
        make_desc(2,11,0,1,1,wx_gold[1]+1'b1);
        make_desc(3,21,1,1,1,hx_gold[1][63:0]+1'b1);
        make_desc(4,12,0,0,0,0);
        make_desc(5,22,1,0,0,0);

        peek(32'h2004,32'h5043494d);
        peek(32'h2030,16);
        poke(32'h2008,DESC_BASE[31:0]);poke(32'h200c,DESC_BASE[63:32]);
        poke(32'h2010,RES_BASE[31:0]);poke(32'h2014,RES_BASE[63:32]);
        poke(32'h2000,1);
        peek(32'h2000,1);
        poke(32'h2018,6);
        begin: wait_dma
            for(integer j=0;j<150000;j++) begin
                @(negedge clk);
                rd_addr=32'h2024;#1;
                if(rd_value==6) disable wait_dma;
            end
            $fatal(1,"PCIM DMA completion timeout cons=%0d prod=%0d fault=%0h",
                dut.dma.descriptor_consumed,dut.dma.completion_produced,dut.dma.fault);
        end
        peek(32'h2020,6);
        peek(32'h2024,6);
        peek(32'h202c,0);
        if(reads!=6 || writes!=6) $fatal(1,"PCIM read/write count %d %d",reads,writes);

        // Results may interleave across algorithms but tags preserve identity.
        for(integer i=0;i<6;i++) begin
            logic [31:0] tag,alg,attempts;
            logic [511:0] result_word;
            result_word=completion_mem[i];
            tag=result_word[31:0];alg=result_word[63:32];
            attempts=result_word[95:64];
            case(tag)
                10: if(alg!==0 || attempts!==2 || result_word[96]!==0)
                    $fatal(1,"HashWX no-hit completion");
                20: if(alg!==1 || attempts!==2 || result_word[96]!==0)
                    $fatal(1,"HashX no-hit completion");
                11: if(alg!==0 || attempts!==1 || result_word[96]!==1
                        || result_word[191:128]!==64'd1
                        || result_word[511:256]!=={192'd0,wx_gold[1]})
                    $fatal(1,"HashWX DMA winner payload incorrect");
                21: if(alg!==1 || attempts!==1 || result_word[96]!==1
                        || result_word[191:128]!==64'd1
                        || result_word[511:256]!==hx_gold[1])
                    $fatal(1,"HashX DMA full digest incorrect");
                12: if(alg!==0 || attempts!==0 || result_word[96]!==0)
                    $fatal(1,"HashWX empty completion");
                22: if(alg!==1 || attempts!==0 || result_word[96]!==0)
                    $fatal(1,"HashX empty completion");
                default: $fatal(1,"unknown DMA tag=%0d",tag);
            endcase
        end
        poke(32'h201c,6);

        // Fill all 16 completion slots with zero-work descriptors,
        // crossing both command and completion physical ring wrap.
        for(integer i=0;i<16;i++)
            make_desc((6+i)%16,32'(100+i),32'(i%2),0,64'(i),0);
        poke(32'h2018,22);
        begin: ring_fill
            for(integer j=0;j<3000;j++) begin
                @(negedge clk);
                rd_addr=32'h2024;#1;
                if(rd_value==22) disable ring_fill;
            end
            $fatal(1,"ring wrap/fill timeout cons=%0d prod=%0d",
                dut.dma.descriptor_consumed,dut.dma.completion_produced);
        end
        peek(32'h2020,22);
        for(integer i=0;i<16;i++) begin
            logic [511:0] record;
            record=completion_mem[(6+i)%16];
            if(record[31:0]!==32'(100+i) ||
                record[63:32]!==32'(i%2) ||
                record[95:64]!==0 || record[96]!==0)
                $fatal(1,"ring wrap completion corrupted at i=%0d",i);
        end
        // Two additional descriptors can enter the hardware FIFOs, but
        // their DMA completions MUST wait for host acknowledgment.
        make_desc(6,200,0,0,0,0);
        make_desc(7,201,1,0,0,0);
        poke(32'h2018,24);
        repeat(200) @(negedge clk);
        peek(32'h2024,22);
        peek(32'h202c,0);
        poke(32'h201c,22); // host frees ring space
        begin: ring_resume
            for(integer j=0;j<3000;j++) begin
                @(negedge clk);
                rd_addr=32'h2024;#1;
                if(rd_value==24) disable ring_resume;
            end
            $fatal(1,"completion ring did not resume after ack");
        end
        if(completion_mem[6][31:0]!==200 ||
           completion_mem[7][31:0]!==201)
            $fatal(1,"resumed DMA completions corrupted");
        peek(32'h2020,24);
        peek(32'h202c,0);
        poke(32'h201c,24);
        poke(32'h2000,0);
        peek(32'h2000,0);
        // Malformed algorithm must halt safely without dispatching or
        // writing any fabricated completion.
        descriptor_mem[0][63:32]=32'd17;
        poke(32'h2000,1);poke(32'h2018,1);
        begin: wait_fault
            for(integer j=0;j<1000;j++) begin
                @(negedge clk);
                rd_addr=32'h202c;#1;
                if(rd_value==5) disable wait_fault;
            end
            $fatal(1,"invalid descriptor was not rejected");
        end
        peek(32'h2020,0);peek(32'h2024,0);
        $display("PASS F1 PCIM DMA: 24 real HashWX/HashX results, ring wrap, full completion backpressure/ack, AXI R/W, malformed descriptor fault");
        $finish;
    end
endmodule
