// HashX v1 execution lane; immutable generated HashXProgram configured by host.
// Parallel 16x16 DSP limbs; 1, 2, 4, 8, or 16 products per cycle.
// SPDX-License-Identifier: LGPL-3.0-only
module hashx_core #(
    parameter integer MUL_PARTIALS = 4,
    parameter bit DIRECT_MUL = 1'b1,
    parameter bit DUAL_ISSUE = 1'b1
) (
    input logic clk, rst,
    input logic cfg_we,
    input logic [10:0] cfg_addr,
    input logic [31:0] cfg_data,
    input logic start,
    input logic [63:0] nonce, target,
    output logic busy, done, hit,
    output logic [255:0] digest
);
    // Instruction [63:60] opcode, [59:57] dst, [56:54] src,
    // [53:48] shift, [31:0] signed immediate or branch mask.
    logic [63:0] microcode [0:511];
    // A separately addressed copy preserves single-read-port BRAM
    // mapping rather than turning the program store into 512x64 LUTRAM
    // when dual issue reads PC and PC+1 concurrently.
    logic [63:0] microcode_next [0:511];
    logic [63:0] key [0:3];
    logic [63:0] regs [0:7];
    logic [255:0] sip_state;
    logic [63:0] current_nonce, current_target;
    logic [9:0] pc, branch_target;
    logic branch_used;
    logic [31:0] mulh_result;
    logic [3:0] init_step;
    logic [127:0] product_acc;
    logic [63:0] mul_a, mul_b;
    logic [3:0] mul_i, mul_op;
    logic [2:0] mul_dst;

    typedef enum logic [2:0] {IDLE, INIT, EXEC, MUL, MUL_DONE, FINISH_A, FINISH_B} state_t;
    state_t state;
    assign busy = state != IDLE;

    function automatic logic [255:0] sipround(input logic [255:0] s);
        logic [63:0] a,b,c,d;
        begin
            {d,c,b,a} = s;
            a = a + b;
            c = c + d;
            b = {b[50:0],b[63:51]};
            d = {d[47:0],d[63:48]};
            b ^= a;
            d ^= c;
            a = {a[31:0],a[63:32]};
            c = c + b;
            a = a + d;
            b = {b[46:0],b[63:47]};
            d = {d[42:0],d[63:43]};
            b ^= c;
            d ^= a;
            c = {c[31:0],c[63:32]};
            sipround = {d,c,b,a};
        end
    endfunction

    wire [63:0] instr = microcode[pc[8:0]];
    wire [3:0] opcode = instr[63:60];
    wire [2:0] dst = instr[59:57];
    wire [2:0] src = instr[56:54];
    wire [5:0] shamt = instr[53:48];
    wire [63:0] imm = {{32{instr[31]}},instr[31:0]};
    wire [63:0] x = regs[dst], y = regs[src];
    // Optional one-cycle 64x64 multiply. Only one unsigned product
    // network is instantiated for all three multiply opcodes; signed
    // high-half correction reuses the same combinational result.
    // It removes the per-multiply FSM stalls, but its critical path
    // must be verified by real FPGA timing closure before deployment.
    wire [127:0] direct_product = x * y;
    wire [63:0] direct_signed_high =
        direct_product[127:64] - (x[63] ? y : 64'd0)
                               - (y[63] ? x : 64'd0);

    // Two-issue fusion is valid only for a multiplication followed
    // immediately by a cheap, independent register ALU instruction.
    // We decode BOTH instructions from the uploaded microcode; nothing
    // about a particular generated program is baked into the hardware.
    // An exact dependency/WAW check guarantees the same state as two
    // sequential instructions, including after taken branches.
    wire [8:0] next_pc = pc[8:0] + 9'd1;
    wire [63:0] next_instr = microcode_next[next_pc];
    wire [3:0] next_opcode = next_instr[63:60];
    wire [2:0] next_dst = next_instr[59:57];
    wire [2:0] next_src = next_instr[56:54];
    wire [5:0] next_shamt = next_instr[53:48];
    wire [63:0] next_imm = {{32{next_instr[31]}},next_instr[31:0]};
    wire [63:0] next_x = regs[next_dst], next_y = regs[next_src];
    wire next_uses_src = next_opcode == 3 || next_opcode == 5 ||
                         next_opcode == 6;
    wire fusion = DIRECT_MUL && DUAL_ISSUE && pc != 511 &&
                  opcode <= 2 && next_opcode >= 3 && next_opcode <= 8 &&
                  dst != next_dst &&
                  !(next_uses_src && next_src == dst);
    // One 64-bit ALU serves scalar instructions and the second
    // independently paired instruction. This avoids duplicating add,
    // subtract, variable shifts and rotate networks for dual issue.
    wire [3:0] alu_op = fusion ? next_opcode : opcode;
    wire [63:0] alu_x = fusion ? next_x : x;
    wire [63:0] alu_y = fusion ? next_y : y;
    wire [63:0] alu_imm = fusion ? next_imm : imm;
    wire [5:0] alu_shamt = fusion ? next_shamt : shamt;
    logic [63:0] alu_value;
    always_comb begin
        case (alu_op)
            3: alu_value = alu_x + (alu_y << alu_shamt);
            4: alu_value = alu_x + alu_imm;
            5: alu_value = alu_x - alu_y;
            6: alu_value = alu_x ^ alu_y;
            7: alu_value = alu_x ^ alu_imm;
            8: alu_value = (alu_x >> alu_shamt) |
                              (alu_x << (7'd64 - {1'b0,alu_shamt}));
            default: alu_value = 0;
        endcase
    end
    // All sixteen 16x16 unsigned limb products are scheduled in
    // MUL_STEPS=16/MUL_PARTIALS groups. The balanced tree avoids an
    // O(MUL_PARTIALS) ripple path for the partial-product reduction.
    wire [127:0] reduce [1:2*MUL_PARTIALS-1];
    for (genvar p = 0; p < MUL_PARTIALS; p++) begin : product
        wire [3:0] index = 4'(mul_i * MUL_PARTIALS + p);
        wire [15:0] a = mul_a[{index[3:2],4'b0} +: 16];
        wire [15:0] b = mul_b[{index[1:0],4'b0} +: 16];
        wire [31:0] ab = a * b;
        wire [6:0] shift_bits = ({5'b0,index[3:2]} +
                                 {5'b0,index[1:0]}) << 4;
        assign reduce[MUL_PARTIALS+p] = {96'b0, ab} << shift_bits;
    end
    for (genvar p = 1; p < MUL_PARTIALS; p++) begin : tree
        assign reduce[p] = reduce[2*p] + reduce[2*p+1];
    end
    wire [127:0] partial = reduce[1];
    localparam integer MUL_STEPS = 16/MUL_PARTIALS;
    wire [63:0] signed_high = product_acc[127:64]
                   - (mul_a[63] ? mul_b : 64'd0)
                   - (mul_b[63] ? mul_a : 64'd0);

    // The init and two final digest SipRounds are mutually exclusive.
    // Reuse one 256-bit round network and its existing sip_state register,
    // avoiding three parallel 64-bit-adder networks per replicated lane.
    wire [255:0] sip_input = state == FINISH_A
        ? {regs[3],regs[2],regs[1]+key[1],regs[0]+key[0]}
        : state == FINISH_B
        ? {regs[7]+key[3],regs[6]+key[2],regs[5],regs[4]}
        : sip_state;
    wire [255:0] sip_output = sipround(sip_input);
    wire [255:0] finalized = sip_state ^ sip_output;

    always_ff @(posedge clk) begin
        if (rst) begin
            state <= IDLE;
            pc <= 0;
            branch_target <= 0;
            branch_used <= 0;
            mulh_result <= 0;
            init_step <= 0;
            product_acc <= 0;
            mul_a <= 0;
            mul_b <= 0;
            mul_i <= 0;
            mul_op <= 0;
            mul_dst <= 0;
            sip_state <= 0;
            current_nonce <= 0;
            current_target <= 0;
            digest <= 0;
            hit <= 0;
            done <= 0;
            for (int j=0;j<4;j++) key[j] <= 0;
            for (int j=0;j<8;j++) regs[j] <= 0;
        end else begin
            if (cfg_we && state == IDLE) begin
                if (cfg_addr < 1024) begin
                    if (!cfg_addr[0]) begin
                        microcode[cfg_addr[9:1]][31:0] <= cfg_data;
                        if (DUAL_ISSUE)
                            microcode_next[cfg_addr[9:1]][31:0] <= cfg_data;
                    end else begin
                        microcode[cfg_addr[9:1]][63:32] <= cfg_data;
                        if (DUAL_ISSUE)
                            microcode_next[cfg_addr[9:1]][63:32] <= cfg_data;
                    end
                end else if (cfg_addr < 1032) begin
                    if (!cfg_addr[0]) key[cfg_addr[2:1]][31:0] <= cfg_data;
                    else key[cfg_addr[2:1]][63:32] <= cfg_data;
                end
            end
            case (state)
                IDLE: if (start && !cfg_we) begin
                    current_nonce <= nonce;
                    current_target <= target;
                    sip_state <= {key[3] ^ nonce, key[2], key[1] ^ 64'hee, key[0]};
                    init_step <= 0;
                    pc <= 0;
                    branch_target <= 0;
                    branch_used <= 0;
                    mulh_result <= 0;
                    done <= 0;
                    hit <= 0;
                    state <= INIT;
                end
                INIT: begin
                    case (init_step)
                        0,1,3,4,5,6,8,9,10,11: sip_state <= sip_output;
                        2: begin
                            sip_state[63:0] <= sip_state[63:0] ^ current_nonce;
                            sip_state[191:128] <= sip_state[191:128] ^ 64'hee;
                        end
                        7: begin
                            regs[0] <= sip_state[63:0];
                            regs[1] <= sip_state[127:64];
                            regs[2] <= sip_state[191:128];
                            regs[3] <= sip_state[255:192];
                            sip_state[127:64] <= sip_state[127:64] ^ 64'hdd;
                        end
                        12: begin
                            regs[4] <= sip_state[63:0];
                            regs[5] <= sip_state[127:64];
                            regs[6] <= sip_state[191:128];
                            regs[7] <= sip_state[255:192];
                        end
                        default: ;
                    endcase
                    if (init_step == 12) state <= EXEC;
                    else init_step <= init_step + 1'b1;
                end
                EXEC: begin
                    case (opcode)
                        0,1,2: begin
                            if (DIRECT_MUL) begin
                                case (opcode)
                                    0: regs[dst] <= direct_product[63:0];
                                    1: begin
                                        regs[dst] <= direct_product[127:64];
                                        mulh_result <= direct_product[95:64];
                                    end
                                    2: begin
                                        regs[dst] <= direct_signed_high;
                                        mulh_result <= direct_signed_high[31:0];
                                    end
                                    default: ;
                                endcase
                            end else begin
                                mul_a <= x;
                                mul_b <= y;
                                mul_dst <= dst;
                                mul_op <= opcode;
                                mul_i <= 0;
                                product_acc <= 0;
                                state <= MUL;
                            end
                        end
                        3,4,5,6,7,8: regs[dst] <= alu_value;
                        9: branch_target <= pc;
                        10: if (!branch_used && (mulh_result & instr[31:0]) == 0) begin
                            pc <= branch_target;
                            branch_used <= 1;
                        end
                        default: ;
                    endcase
                    if (DIRECT_MUL || (opcode != 0 && opcode != 1 && opcode != 2)) begin
                        if (opcode != 10 || branch_used || (mulh_result & instr[31:0]) != 0) begin
                            if (fusion) regs[next_dst] <= alu_value;
                            if (pc == 511 || (fusion && pc == 510))
                                state <= FINISH_A;
                            else begin
                                pc <= pc + (fusion ? 10'd2 : 10'd1);
                                state <= EXEC;
                            end
                        end
                    end
                end
                MUL: begin
                    product_acc <= product_acc + partial;
                    if (mul_i == 4'(MUL_STEPS-1)) state <= MUL_DONE;
                    else mul_i <= mul_i + 1'b1;
                end
                MUL_DONE: begin
                    if (mul_op == 0) regs[mul_dst] <= product_acc[63:0];
                    else if (mul_op == 1) begin
                        regs[mul_dst] <= product_acc[127:64];
                        mulh_result <= product_acc[95:64];
                    end else begin
                        regs[mul_dst] <= signed_high;
                        mulh_result <= signed_high[31:0];
                    end
                    if (pc == 511) state <= FINISH_A;
                    else begin pc <= pc + 1'b1; state <= EXEC; end
                end
                FINISH_A: begin
                    sip_state <= sip_output;
                    state <= FINISH_B;
                end
                FINISH_B: begin
                    digest <= finalized;
                    hit <= finalized[63:0] < current_target;
                    done <= 1;
                    state <= IDLE;
                end
                default: state <= IDLE;
            endcase
        end
    end
endmodule
