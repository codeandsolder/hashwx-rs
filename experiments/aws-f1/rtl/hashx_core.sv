// HashX v1 execution lane; immutable generated HashXProgram configured by host.
// Single 16x16 DSP-friendly multiplier accumulates 64x64=>128 in 16 cycles.
// SPDX-License-Identifier: LGPL-3.0-only
module hashx_core(
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

    typedef enum logic [2:0] {IDLE, INIT, EXEC, MUL, MUL_DONE, FINISH} state_t;
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

    // One 16x16 unsigned multiplier, executed for all 16 limb pairs.
    wire [1:0] row = mul_i[3:2];
    wire [1:0] col = mul_i[1:0];
    wire [15:0] limb_a = mul_a[{row,4'b0} +: 16];
    wire [15:0] limb_b = mul_b[{col,4'b0} +: 16];
    wire [31:0] limb_product = limb_a * limb_b;
    wire [6:0] partial_shift = ({5'b0,row} + {5'b0,col}) << 4;
    wire [127:0] partial = {96'b0,limb_product} << partial_shift;
    wire [63:0] signed_high = product_acc[127:64]
                   - (mul_a[63] ? mul_b : 64'd0)
                   - (mul_b[63] ? mul_a : 64'd0);

    wire [255:0] x_mix = sipround({
        regs[3],regs[2],regs[1]+key[1],regs[0]+key[0]
    });
    wire [255:0] y_mix = sipround({
        regs[7]+key[3],regs[6]+key[2],regs[5],regs[4]
    });
    wire [255:0] finalized = x_mix ^ y_mix;

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
                    if (!cfg_addr[0]) microcode[cfg_addr[9:1]][31:0] <= cfg_data;
                    else microcode[cfg_addr[9:1]][63:32] <= cfg_data;
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
                        0,1,3,4,5,6,8,9,10,11: sip_state <= sipround(sip_state);
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
                            mul_a <= x;
                            mul_b <= y;
                            mul_dst <= dst;
                            mul_op <= opcode;
                            mul_i <= 0;
                            product_acc <= 0;
                            state <= MUL;
                        end
                        3: regs[dst] <= x + (y << shamt);
                        4: regs[dst] <= x + imm;
                        5: regs[dst] <= x - y;
                        6: regs[dst] <= x ^ y;
                        7: regs[dst] <= x ^ imm;
                        8: regs[dst] <= (x >> shamt) | (x << (7'd64 - {1'b0,shamt}));
                        9: branch_target <= pc;
                        10: if (!branch_used && (mulh_result & instr[31:0]) == 0) begin
                            pc <= branch_target;
                            branch_used <= 1;
                        end
                        default: ;
                    endcase
                    if (opcode != 0 && opcode != 1 && opcode != 2) begin
                        if (opcode != 10 || branch_used || (mulh_result & instr[31:0]) != 0) begin
                            if (pc == 511) state <= FINISH;
                            else begin pc <= pc + 1'b1; state <= EXEC; end
                        end
                    end
                end
                MUL: begin
                    product_acc <= product_acc + partial;
                    if (mul_i == 15) state <= MUL_DONE;
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
                    if (pc == 511) state <= FINISH;
                    else begin pc <= pc + 1'b1; state <= EXEC; end
                end
                FINISH: begin
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
