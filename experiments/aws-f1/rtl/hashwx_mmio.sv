// HashWX v2 BAR0 endpoint: queued commands, retained completion FIFO.
// All offsets are bytes; the shell owns AXI-Lite handshaking.
// SPDX-License-Identifier: LGPL-3.0-only
module hashwx_mmio #(
    parameter integer LANES = 4,
    parameter integer QUEUE_DEPTH = 4
)(
    input logic clk, rst, wr_en,
    input logic [31:0] wr_addr, wr_data, rd_addr,
    output logic [31:0] rd_value,
    input logic dma_active, dma_command_valid, dma_result_pop,
    output logic dma_command_ready, dma_result_valid, dma_result_found,
    input logic [63:0] dma_command_nonce, dma_command_target,
    input logic [31:0] dma_command_count, dma_command_tag,
    output logic [31:0] dma_result_tag, dma_result_attempts,
    output logic [63:0] dma_result_nonce,
    output logic [63:0] dma_result_hash,
    output logic dma_config_idle
);
    logic [63:0] first_nonce, target;
    logic [31:0] nonce_count, staged_tag;
    logic overflow_error, program_error;
    wire queue_idle, command_ready, completion_valid, completion_found;
    wire [31:0] completed_tag, completed_attempts;
    wire [63:0] completed_nonce, completed_hash;
    wire [15:0] queued_commands, queued_results;
    wire scanner_start, scanner_busy, scanner_done, scanner_found;
    wire [31:0] scanner_count, scanner_attempts;
    wire [63:0] scanner_nonce, scanner_target, scanner_winner_nonce, scanner_winner_hash;

    wire mmio_command = wr_en && wr_addr == 32'h604 && wr_data[0];
    wire command_valid = dma_active ? dma_command_valid : mmio_command;
    assign dma_command_ready = dma_active && command_ready;
    wire result_pop = dma_active ? dma_result_pop :
        (wr_en && wr_addr == 32'h63c && wr_data[0]);
    assign dma_result_valid = completion_valid;
    assign dma_result_found = completion_found;
    assign dma_result_tag = completed_tag;
    assign dma_result_attempts = completed_attempts;
    assign dma_result_nonce = completed_nonce;
    assign dma_result_hash = completed_hash;
    assign dma_config_idle = queue_idle;
    wire config_write = wr_en && wr_addr >= 32'h800 &&
        wr_addr < 32'ha10 && wr_addr[1:0] == 0;
    wire cfg_we = config_write && queue_idle && !dma_active;

    hashwx_scan #(.LANES(LANES)) scanner(
        .clk(clk), .rst(rst),
        .cfg_we(cfg_we), .cfg_addr(wr_addr[9:2]), .cfg_data(wr_data),
        .start(scanner_start), .first_nonce(scanner_nonce),
        .target(scanner_target), .nonce_count(scanner_count),
        .busy(scanner_busy), .done(scanner_done), .found(scanner_found),
        .attempts(scanner_attempts),
        .winner_nonce(scanner_winner_nonce), .winner_hash(scanner_winner_hash)
    );
    hash_job_queue #(.HASH_BITS(64), .DEPTH(QUEUE_DEPTH)) queue(
        .clk(clk), .rst(rst),
        .command_valid(command_valid), .command_ready(command_ready),
        .command_nonce(dma_active ? dma_command_nonce : first_nonce),
        .command_target(dma_active ? dma_command_target : target),
        .command_count(dma_active ? dma_command_count : nonce_count),
        .command_tag(dma_active ? dma_command_tag : staged_tag),
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
            if (mmio_command && (dma_active || !command_ready))
                overflow_error <= 1;
            if (config_write && (!queue_idle || dma_active))
                program_error <= 1;
            case (wr_addr)
                32'h608: first_nonce[31:0] <= wr_data;
                32'h60c: first_nonce[63:32] <= wr_data;
                32'h610: target[31:0] <= wr_data;
                32'h614: target[63:32] <= wr_data;
                32'h618: nonce_count <= wr_data;
                32'h644: staged_tag <= wr_data;
                32'h64c: begin
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
            32'h600: rd_value = 32'h48585732; // HXW2
            32'h608: rd_value = first_nonce[31:0];
            32'h60c: rd_value = first_nonce[63:32];
            32'h610: rd_value = target[31:0];
            32'h614: rd_value = target[63:32];
            32'h618: rd_value = nonce_count;
            // bit0 !idle, bit1 completion, bit2 winner, bit3 can enqueue,
            // bit4 overflow reject, bit5 config-write reject
            32'h61c: rd_value = 32'(!queue_idle) |
                (32'(completion_valid) << 1) |
                (32'(completion_found) << 2) |
                (32'(command_ready) << 3) |
                (32'(overflow_error) << 4) |
                (32'(program_error) << 5);
            32'h620: rd_value = completed_attempts;
            32'h624: rd_value = completed_nonce[31:0];
            32'h628: rd_value = completed_nonce[63:32];
            32'h62c: rd_value = completed_hash[31:0];
            32'h630: rd_value = completed_hash[63:32];
            32'h634: rd_value = 32'(LANES);
            32'h638: rd_value = completed_tag;
            32'h640: rd_value = {queued_results,queued_commands};
            32'h644: rd_value = staged_tag;
            32'h648: rd_value = 32'(QUEUE_DEPTH);
            32'h64c: rd_value = {30'b0,program_error,overflow_error};
            default: ;
        endcase
    end
endmodule
