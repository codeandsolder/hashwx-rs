// HashX v1 execution lane; immutable generated HashXProgram configured by host.
// Parallel 16x16 DSP limbs; 1, 2, 4, 8, or 16 products per cycle.
// SPDX-License-Identifier: LGPL-3.0-only
module hashx_core #(
    parameter integer MUL_PARTIALS = 4,
    parameter bit DIRECT_MUL = 1'b1,
    parameter bit PIPE_MUL = 1'b0
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
    // One program-memory read serves normal and statically fused micro-ops.
    // Host encodes independent multiply+ALU pairs into opcodes 11-13.
    logic [9:0] program_len;
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

    // For pipelined multiply, advance PC in EXEC so MUL_DONE reads
    // the following instruction using the original registered PC path.
    // This keeps the single-port block-RAM inference; an asynchronous
    // mux in front of the read address instead maps it into distributed
    // RAM, a very expensive substitution on UltraScale+.
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
    // Optional two-stage 64x64 multiplier: compute four independent
    // unsigned 32x32 products, register them, and combine them in
    // MUL_DONE. Unlike the single-cycle path this doesn't put DSP
    // multiplication AND carry-chain accumulation on one timing arc.
    // The 64x64 unsigned product is
    // p00 + ((p01 + p10) << 32) + (p11 << 64).
    logic [63:0] mul_p00, mul_p01, mul_p10, mul_p11;
    wire [63:0] pp00 = x[31:0] * y[31:0];
    wire [63:0] pp01 = x[31:0] * y[63:32];
    wire [63:0] pp10 = x[63:32] * y[31:0];
    wire [63:0] pp11 = x[63:32] * y[63:32];
    wire [127:0] direct_product = x * y;
    wire [63:0] direct_signed_high = direct_product[127:64]
        - (x[63] ? y : 64'd0) - (y[63] ? x : 64'd0);
    wire [65:0] mid = {34'd0, mul_p00[63:32]}
                    + {2'b00, mul_p01}
                    + {2'b00, mul_p10};
    wire [63:0] pipe_low = {mid[31:0], mul_p00[31:0]};
    wire [63:0] pipe_high = mul_p11 + 64'(mid[65:32]);
    wire [63:0] pipe_signed_high = pipe_high
        - (mul_a[63] ? mul_b : 64'd0)
        - (mul_b[63] ? mul_a : 64'd0);

    // Compact fused multiply/ALU opcodes 11,12,13 encode respectively
    // MUL, UMULH, SMULH followed by one independent op 3..8.
    // Static host fusion performs all RAW and WAW dependency checks.
    wire fusion = opcode >= 11 && opcode <= 13;
    wire mul_opcode_valid = opcode <= 2 || fusion;
    wire [1:0] mul_kind = fusion ? 2'(opcode - 4'd11) : opcode[1:0];
    wire [3:0] next_opcode = instr[53:50];
    wire [2:0] next_dst = instr[49:47];
    wire [2:0] next_src = instr[46:44];
    wire [5:0] next_shamt = instr[43:38];
    wire at_end = pc == program_len - 10'd1;
    wire alu_reads_src = opcode == 3 || opcode == 5 || opcode == 6;
    wire pipe_slot = DIRECT_MUL && PIPE_MUL && state == MUL_DONE &&
        pc < program_len && opcode >= 3 && opcode <= 8 &&
        dst != mul_dst && !(alu_reads_src && src == mul_dst);
    // One 64-bit ALU serves scalar instructions and the second
    // independently paired instruction. This avoids duplicating add,
    // subtract, variable shifts and rotate networks for dual issue.
    wire [3:0] alu_op = fusion ? next_opcode : opcode;
    // Select register ADDRESSES before the register-file read muxes.
    // This avoids reading both candidate register pairs then muxing
    // two 64-bit operands after the read.
    wire [2:0] alu_dst = fusion ? next_dst : dst;
    wire [2:0] alu_src = fusion ? next_src : src;
    wire [63:0] alu_x = regs[alu_dst], alu_y = regs[alu_src];
    wire [31:0] alu_imm32 = fusion ? instr[37:6] : instr[31:0];
    wire [63:0] alu_imm = {{32{alu_imm32[31]}},alu_imm32};
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
            program_len <= 0;
            branch_target <= 0;
            branch_used <= 0;
            mulh_result <= 0;
            init_step <= 0;
            product_acc <= 0;
            mul_p00 <= 0;
            mul_p01 <= 0;
            mul_p10 <= 0;
            mul_p11 <= 0;
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
                    if (!cfg_addr[0]) microcode[cfg_addr[9:1]][31:0] <= cfg_data;
                    else microcode[cfg_addr[9:1]][63:32] <= cfg_data;
                end else if (cfg_addr < 1032) begin
                    if (!cfg_addr[0]) key[cfg_addr[2:1]][31:0] <= cfg_data;
                    else key[cfg_addr[2:1]][63:32] <= cfg_data;
                end else if (cfg_addr == 1032) begin
                    program_len <= cfg_data[9:0];
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
                        0,1,2,11,12,13: begin
                            if (DIRECT_MUL && PIPE_MUL) begin
                                mul_p00 <= pp00;
                                mul_p01 <= pp01;
                                mul_p10 <= pp10;
                                mul_p11 <= pp11;
                                mul_a <= x;
                                mul_b <= y;
                                mul_dst <= dst;
                                mul_op <= {2'b00,mul_kind};
                                if (fusion) regs[next_dst] <= alu_value;
                                // At the following edge PC is the next
                                // instruction, or program_len when the
                                // multiply was the final instruction.
                                pc <= pc + 10'd1;
                                state <= MUL_DONE;
                            end else if (DIRECT_MUL) begin
                                case (mul_kind)
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
                                if (fusion) regs[next_dst] <= alu_value;
                            end else begin
                                mul_a <= x;
                                mul_b <= y;
                                mul_dst <= dst;
                                mul_op <= {2'b00,mul_kind};
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
                    if ((DIRECT_MUL && !PIPE_MUL) || !mul_opcode_valid) begin
                        if (opcode != 10 || branch_used || (mulh_result & instr[31:0]) != 0) begin
                            if (at_end) state <= FINISH_A;
                            else begin pc <= pc + 10'd1; state <= EXEC; end
                        end
                    end
                end
                MUL: begin
                    product_acc <= product_acc + partial;
                    if (mul_i == 4'(MUL_STEPS-1)) state <= MUL_DONE;
                    else mul_i <= mul_i + 1'b1;
                end
                MUL_DONE: begin
                    if (DIRECT_MUL && PIPE_MUL) begin
                        case (mul_op)
                            0: regs[mul_dst] <= pipe_low;
                            1: begin
                                regs[mul_dst] <= pipe_high;
                                mulh_result <= pipe_high[31:0];
                            end
                            2: begin
                                regs[mul_dst] <= pipe_signed_high;
                                mulh_result <= pipe_signed_high[31:0];
                            end
                            default: ;
                        endcase
                        // Correct even for arbitrary host-uploaded programs:
                        // recheck RAW/WAW hazards against the pending
                        // multiply destination before committing the
                        // next scalar instruction in this clock.
                        if (pipe_slot) regs[dst] <= alu_value;
                    end else begin
                        if (mul_op == 0) regs[mul_dst] <= product_acc[63:0];
                        else if (mul_op == 1) begin
                            regs[mul_dst] <= product_acc[127:64];
                            mulh_result <= product_acc[95:64];
                        end else begin
                            regs[mul_dst] <= signed_high;
                            mulh_result <= signed_high[31:0];
                        end
                        if (fusion) regs[next_dst] <= alu_value;
                    end
                    if (DIRECT_MUL && PIPE_MUL) begin
                        if (pc >= program_len || (pipe_slot && at_end))
                            state <= FINISH_A;
                        else begin
                            if (pipe_slot) pc <= pc + 10'd1;
                            state <= EXEC;
                        end
                    end else if (at_end) state <= FINISH_A;
                    else begin pc <= pc + 10'd1; state <= EXEC; end
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
