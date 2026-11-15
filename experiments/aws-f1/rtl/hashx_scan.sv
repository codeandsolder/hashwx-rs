// Public HashX v1 scan interface, backed by the parameterized wave scheduler.
// SPDX-License-Identifier: LGPL-3.0-only
module hashx_scan #(
    parameter integer LANES = 4
)(
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
    hash_parallel_scan #(.LANES(LANES), .IS_HASHX(1'b1), .HASH_BITS(256)) scheduler (
        .clk(clk), .rst(rst),
        .cfg_we(cfg_we), .cfg_addr(cfg_addr), .cfg_data(cfg_data),
        .start(start), .first_nonce(first_nonce), .target(target),
        .nonce_count(nonce_count),
        .busy(busy), .done(done), .found(found), .attempts(attempts),
        .winner_nonce(winner_nonce), .winner_hash(winner_digest)
    );
endmodule
