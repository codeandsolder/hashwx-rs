// Single-lane HashX v1 bounded nonce scan. Compare first (LE) digest word.
// SPDX-License-Identifier: LGPL-3.0-only
module hashx_scan(
    input logic clk, rst,
    input logic cfg_we,
    input logic [10:0] cfg_addr,
    input logic [31:0] cfg_data,
    input logic start,
    input logic [63:0] first_nonce, target,
    input logic [31:0] nonce_count,
    output logic busy, done, found,
    output logic [31:0] attempts,
    output logic [63:0] winner_nonce,
    output logic [255:0] winner_digest
);
    typedef enum logic [1:0] { IDLE, LAUNCH, WAIT_BUSY, WAIT_RESULT } state_t;
    state_t state;
    logic core_start, core_busy, core_done, core_hit;
    logic [63:0] current_nonce, active_target;
    logic [255:0] core_digest;
    logic [31:0] remaining;
    hashx_core core(
        .clk(clk), .rst(rst),
        .cfg_we(cfg_we && !busy), .cfg_addr(cfg_addr), .cfg_data(cfg_data),
        .start(core_start), .nonce(current_nonce), .target(active_target),
        .busy(core_busy), .done(core_done), .hit(core_hit), .digest(core_digest)
    );
    assign busy = state != IDLE;
    always_ff @(posedge clk) begin
        if (rst) begin
            state <= IDLE;
            core_start <= 0;
            current_nonce <= 0;
            active_target <= 0;
            remaining <= 0;
            done <= 0;
            found <= 0;
            attempts <= 0;
            winner_nonce <= 0;
            winner_digest <= 0;
        end else case (state)
            IDLE: begin
                core_start <= 0;
                if (start && !cfg_we) begin
                    current_nonce <= first_nonce;
                    active_target <= target;
                    remaining <= nonce_count;
                    attempts <= 0;
                    done <= 0;
                    found <= 0;
                    winner_nonce <= 0;
                    winner_digest <= 0;
                    if (nonce_count == 0) done <= 1;
                    else state <= LAUNCH;
                end
            end
            LAUNCH: begin
                core_start <= 1;
                state <= WAIT_BUSY;
            end
            WAIT_BUSY: begin
                core_start <= 0;
                if (core_busy) state <= WAIT_RESULT;
            end
            WAIT_RESULT: if (core_done) begin
                attempts <= attempts + 1'b1;
                if (core_hit) begin
                    found <= 1;
                    winner_nonce <= current_nonce;
                    winner_digest <= core_digest;
                    done <= 1;
                    state <= IDLE;
                end else if (remaining == 1) begin
                    done <= 1;
                    state <= IDLE;
                end else begin
                    remaining <= remaining - 1'b1;
                    current_nonce <= current_nonce + 1'b1;
                    state <= LAUNCH;
                end
            end
            default: state <= IDLE;
        endcase
    end
endmodule
