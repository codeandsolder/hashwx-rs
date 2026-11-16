// Wave-scheduled parallel nonce search for HashWX 1.0 or HashX v1.
// Every lane receives the same seed program (broadcast configuration).
// Results are committed only when the complete current wave finishes:
// a qualifying nonce at the lowest offset always wins, independent of
// individual-lane latency, while actual attempts count all issued candidates.
// SPDX-License-Identifier: LGPL-3.0-only
module hash_parallel_scan #(
    parameter integer LANES = 4,
    parameter integer HX_MUL_PARTIALS = 4,
    parameter bit HX_DIRECT_MUL = 1'b1,
    parameter bit IS_HASHX = 1'b0,
    parameter integer HASH_BITS = IS_HASHX ? 256 : 64
)(
    input  logic clk, rst,
    input  logic cfg_we,
    input  logic [10:0] cfg_addr,
    input  logic [31:0] cfg_data,
    input  logic start,
    input  logic [63:0] first_nonce, target,
    input  logic [31:0] nonce_count,
    output logic busy, done, found,
    output logic [31:0] attempts,
    output logic [63:0] winner_nonce,
    output logic [HASH_BITS-1:0] winner_hash
);
    typedef enum logic [1:0] {
        IDLE, LAUNCH, WAIT_BUSY, WAIT_RESULTS
    } state_t;

    state_t state;
    logic [63:0] batch_nonce, active_target;
    logic [31:0] remaining;
    logic [LANES-1:0] lane_valid, core_busy, core_done, core_hit;
    wire [HASH_BITS-1:0] core_hash [0:LANES-1];
    logic wave_found;
    logic [63:0] wave_nonce;
    logic [HASH_BITS-1:0] wave_hash;

    assign busy = state != IDLE;

    for (genvar i = 0; i < LANES; i = i + 1) begin : lane
        localparam logic [63:0] NONCE_OFFSET = 64'(i);
        localparam logic [31:0] LANE_INDEX = 32'(i);
        wire [63:0] lane_nonce = batch_nonce + NONCE_OFFSET;
        wire lane_start = (state == LAUNCH) && lane_valid[i];
        assign lane_valid[i] = remaining > LANE_INDEX;

        if (IS_HASHX) begin : x
            hashx_core #(.MUL_PARTIALS(HX_MUL_PARTIALS),
                           .DIRECT_MUL(HX_DIRECT_MUL)) core(
                .clk(clk), .rst(rst),
                .cfg_we(cfg_we && !busy), .cfg_addr(cfg_addr),
                .cfg_data(cfg_data), .start(lane_start),
                .nonce(lane_nonce), .target(active_target),
                .busy(core_busy[i]), .done(core_done[i]),
                .hit(core_hit[i]), .digest(core_hash[i])
            );
        end else begin : wx
            hashwx_core core(
                .clk(clk), .rst(rst),
                .cfg_we(cfg_we && !busy), .cfg_addr(cfg_addr[7:0]),
                .cfg_data(cfg_data), .start(lane_start),
                .nonce(lane_nonce), .target(active_target),
                .busy(core_busy[i]), .done(core_done[i]),
                .hit(core_hit[i]), .hash(core_hash[i])
            );
        end
    end

    // Priority is nonce order, not finish order. Every earlier wave is
    // exhausted before any later wave can be dispatched.
    always_comb begin
        wave_found = 1'b0;
        wave_nonce = '0;
        wave_hash = '0;
        for (integer i = 0; i < LANES; i = i + 1) begin
            if (lane_valid[i] && core_hit[i] && !wave_found) begin
                wave_found = 1'b1;
                wave_nonce = batch_nonce + 64'(i);
                wave_hash = core_hash[i];
            end
        end
    end

    wire all_running = &(core_busy | ~lane_valid);
    wire all_complete = &(core_done | ~lane_valid);
    localparam logic [31:0] LANES32 = 32'(LANES);
    wire [31:0] wave_count =
        remaining < LANES32 ? remaining : LANES32;

    always_ff @(posedge clk) begin
        if (rst) begin
            state <= IDLE;
            batch_nonce <= 0;
            active_target <= 0;
            remaining <= 0;
            done <= 0;
            found <= 0;
            attempts <= 0;
            winner_nonce <= 0;
            winner_hash <= '0;
        end else case (state)
            IDLE: if (start && !cfg_we) begin
                batch_nonce <= first_nonce;
                active_target <= target;
                remaining <= nonce_count;
                attempts <= 0;
                done <= nonce_count == 0;
                found <= 0;
                winner_nonce <= 0;
                winner_hash <= '0;
                if (nonce_count != 0) state <= LAUNCH;
            end
            LAUNCH: state <= WAIT_BUSY;
            WAIT_BUSY: if (all_running) state <= WAIT_RESULTS;
            WAIT_RESULTS: if (all_complete) begin
                attempts <= attempts + wave_count;
                if (wave_found) begin
                    found <= 1;
                    winner_nonce <= wave_nonce;
                    winner_hash <= wave_hash;
                    done <= 1;
                    state <= IDLE;
                end else if (remaining <= LANES32) begin
                    done <= 1;
                    state <= IDLE;
                end else begin
                    remaining <= remaining - LANES32;
                    batch_nonce <= batch_nonce + 64'(LANES);
                    state <= LAUNCH;
                end
            end
            default: state <= IDLE;
        endcase
    end
endmodule
