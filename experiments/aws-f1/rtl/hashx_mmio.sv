// HashX v2 BAR0 endpoint: queued commands, retained full-digest results.
// SPDX-License-Identifier: LGPL-3.0-only
module hashx_mmio #(
    parameter integer LANES = 4,
    parameter integer QUEUE_DEPTH = 4
)(
    input logic clk, rst, wr_en,
    input logic [31:0] wr_addr, wr_data, rd_addr,
    output logic [31:0] rd_value
);
    logic [63:0] first_nonce, target;
    logic [31:0] nonce_count, staged_tag;
    logic overflow_error, program_error;
    wire queue_idle, command_ready, completion_valid, completion_found;
    wire [31:0] completed_tag, completed_attempts;
    wire [63:0] completed_nonce;
    wire [255:0] completed_hash;
    wire [15:0] queued_commands, queued_results;
    wire scanner_start, scanner_busy, scanner_done, scanner_found;
    wire [31:0] scanner_count, scanner_attempts;
    wire [63:0] scanner_nonce, scanner_target, scanner_winner_nonce;
    wire [255:0] scanner_winner_hash;

    wire command_valid = wr_en && wr_addr == 32'h1004 && wr_data[0];
    wire result_pop = wr_en && wr_addr == 32'h1054 && wr_data[0];
    wire config_write = wr_en && wr_addr >= 32'h4000 &&
        wr_addr < 32'h5020 && wr_addr[1:0] == 0;
    wire cfg_we = config_write && queue_idle;

    hashx_scan #(.LANES(LANES)) scanner(
        .clk(clk), .rst(rst),
        .cfg_we(cfg_we), .cfg_addr(wr_addr[12:2]), .cfg_data(wr_data),
        .start(scanner_start), .first_nonce(scanner_nonce),
        .target(scanner_target), .nonce_count(scanner_count),
        .busy(scanner_busy), .done(scanner_done), .found(scanner_found),
        .attempts(scanner_attempts),
        .winner_nonce(scanner_winner_nonce), .winner_digest(scanner_winner_hash)
    );
    hash_job_queue #(.HASH_BITS(256), .DEPTH(QUEUE_DEPTH)) queue(
        .clk(clk), .rst(rst),
        .command_valid(command_valid), .command_ready(command_ready),
        .command_nonce(first_nonce), .command_target(target),
        .command_count(nonce_count), .command_tag(staged_tag),
        .config_idle(queue_idle),
        .scanner_start(scanner_start), .scanner_nonce(scanner_nonce),
        .scanner_target(scanner_target), .scanner_count(scanner_count),
        .scanner_busy(scanner_busy), .scanner_done(scanner_done),
        .scanner_found(scanner_found), .scanner_attempts(scanner_attempts),
        .scanner_winner_nonce(scanner_winner_nonce),
        .scanner_winner_hash(scanner_winner_hash),
        .result_pop(result_pop), .result_valid(completion_valid),
        .result_found(completion_found), .result_tag(completed_tag),
        .result_attempts(completed_attempts),
        .result_nonce(completed_nonce), .result_hash(completed_hash),
        .queued_commands(queued_commands), .queued_results(queued_results)
    );

    always_ff @(posedge clk) begin
        if (rst) begin
            first_nonce <= 0;
            target <= 0;
            nonce_count <= 0;
            staged_tag <= 0;
            overflow_error <= 0;
            program_error <= 0;
        end else if (wr_en) begin
            if (command_valid && !command_ready) overflow_error <= 1;
            if (config_write && !queue_idle) program_error <= 1;
            case (wr_addr)
                32'h1008: first_nonce[31:0] <= wr_data;
                32'h100c: first_nonce[63:32] <= wr_data;
                32'h1010: target[31:0] <= wr_data;
                32'h1014: target[63:32] <= wr_data;
                32'h1018: nonce_count <= wr_data;
                32'h105c: staged_tag <= wr_data;
                32'h1064: begin
                    if (wr_data[0]) overflow_error <= 0;
                    if (wr_data[1]) program_error <= 0;
                end
                default: ;
            endcase
        end
    end

    always_comb begin
        rd_value = 32'hdeadc0de;
        case (rd_addr)
            32'h1000: rd_value = 32'h48585832; // HXX2
            32'h1008: rd_value = first_nonce[31:0];
            32'h100c: rd_value = first_nonce[63:32];
            32'h1010: rd_value = target[31:0];
            32'h1014: rd_value = target[63:32];
            32'h1018: rd_value = nonce_count;
            32'h101c: rd_value = 32'(!queue_idle) |
                (32'(completion_valid) << 1) |
                (32'(completion_found) << 2) |
                (32'(command_ready) << 3) |
                (32'(overflow_error) << 4) |
                (32'(program_error) << 5);
            32'h1020: rd_value = completed_attempts;
            32'h1024: rd_value = completed_nonce[31:0];
            32'h1028: rd_value = completed_nonce[63:32];
            32'h102c: rd_value = completed_hash[31:0];
            32'h1030: rd_value = completed_hash[63:32];
            32'h1034: rd_value = completed_hash[95:64];
            32'h1038: rd_value = completed_hash[127:96];
            32'h103c: rd_value = completed_hash[159:128];
            32'h1040: rd_value = completed_hash[191:160];
            32'h1044: rd_value = completed_hash[223:192];
            32'h1048: rd_value = completed_hash[255:224];
            32'h104c: rd_value = 32'(LANES);
            32'h1050: rd_value = completed_tag;
            32'h1058: rd_value = {queued_results,queued_commands};
            32'h105c: rd_value = staged_tag;
            32'h1060: rd_value = 32'(QUEUE_DEPTH);
            32'h1064: rd_value = {30'b0,program_error,overflow_error};
            default: ;
        endcase
    end
endmodule
