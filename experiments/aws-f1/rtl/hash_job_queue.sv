// Ordered BAR0-command/result FIFOs around one multi-lane scanner.
// Command and result queues are independent, avoiding host round trips
// between consecutive scans. Each result is retained until acknowledged.
// Program upload is permitted only when both FIFOs and scanner are empty.
// SPDX-License-Identifier: LGPL-3.0-only
module hash_job_queue #(
    parameter integer HASH_BITS = 64,
    parameter integer DEPTH = 4
)(
    input  logic clk, rst,
    input  logic command_valid,
    output logic command_ready,
    input  logic [63:0] command_nonce, command_target,
    input  logic [31:0] command_count, command_tag,
    output logic config_idle,
    output logic scanner_start,
    output logic [63:0] scanner_nonce, scanner_target,
    output logic [31:0] scanner_count,
    input  logic scanner_busy, scanner_done, scanner_found,
    input  logic [31:0] scanner_attempts,
    input  logic [63:0] scanner_winner_nonce,
    input  logic [HASH_BITS-1:0] scanner_winner_hash,
    input  logic result_pop,
    output logic result_valid, result_found,
    output logic [31:0] result_tag, result_attempts,
    output logic [63:0] result_nonce,
    output logic [HASH_BITS-1:0] result_hash,
    output logic [15:0] queued_commands, queued_results
);
    localparam integer PTR_BITS = DEPTH > 1 ? $clog2(DEPTH) : 1;
    typedef enum logic [1:0] {
        IDLE, WAIT_ARM, WAIT_DONE
    } state_t;
    state_t state;
    logic [PTR_BITS-1:0] command_wptr, command_rptr, result_wptr, result_rptr;
    logic [15:0] command_used, result_used;
    logic [63:0] cmd_nonce_ram [0:DEPTH-1], cmd_target_ram [0:DEPTH-1];
    logic [31:0] cmd_count_ram [0:DEPTH-1], cmd_tag_ram [0:DEPTH-1];
    logic res_found_ram [0:DEPTH-1];
    logic [31:0] res_tag_ram [0:DEPTH-1], res_attempts_ram [0:DEPTH-1];
    logic [63:0] res_nonce_ram [0:DEPTH-1];
    logic [HASH_BITS-1:0] res_hash_ram [0:DEPTH-1];
    logic [31:0] active_tag;

    function automatic logic [PTR_BITS-1:0] step_ptr(
        input logic [PTR_BITS-1:0] ptr
    );
        if (ptr == PTR_BITS'(DEPTH-1)) step_ptr = '0;
        else step_ptr = ptr + 1'b1;
    endfunction

    assign command_ready = command_used < 16'(DEPTH);
    assign result_valid = result_used != 0;
    assign queued_commands = command_used;
    assign queued_results = result_used;
    assign config_idle = state == IDLE && command_used == 0 && result_used == 0;
    assign result_tag = result_valid ? res_tag_ram[result_rptr] : '0;
    assign result_attempts = result_valid ? res_attempts_ram[result_rptr] : '0;
    assign result_found = result_valid && res_found_ram[result_rptr];
    assign result_nonce = result_valid ? res_nonce_ram[result_rptr] : '0;
    assign result_hash = result_valid ? res_hash_ram[result_rptr] : '0;

    wire push_command = command_valid && command_ready;
    wire pop_command = state == IDLE && command_used != 0 &&
                       result_used < 16'(DEPTH);
    wire push_result = state == WAIT_DONE && scanner_done;
    wire pop_result = result_pop && result_valid;

    always_ff @(posedge clk) begin
        if (rst) begin
            state <= IDLE;
            scanner_start <= 0;
            scanner_nonce <= 0;
            scanner_target <= 0;
            scanner_count <= 0;
            active_tag <= 0;
            command_wptr <= 0;
            command_rptr <= 0;
            result_wptr <= 0;
            result_rptr <= 0;
            command_used <= 0;
            result_used <= 0;
        end else begin
            scanner_start <= 0;

            if (push_command) begin
                cmd_nonce_ram[command_wptr] <= command_nonce;
                cmd_target_ram[command_wptr] <= command_target;
                cmd_count_ram[command_wptr] <= command_count;
                cmd_tag_ram[command_wptr] <= command_tag;
                command_wptr <= step_ptr(command_wptr);
            end
            if (pop_command) begin
                scanner_nonce <= cmd_nonce_ram[command_rptr];
                scanner_target <= cmd_target_ram[command_rptr];
                scanner_count <= cmd_count_ram[command_rptr];
                active_tag <= cmd_tag_ram[command_rptr];
                command_rptr <= step_ptr(command_rptr);
            end
            case ({push_command, pop_command})
                2'b10: command_used <= command_used + 1'b1;
                2'b01: command_used <= command_used - 1'b1;
                default: ;
            endcase

            if (push_result) begin
                res_tag_ram[result_wptr] <= active_tag;
                res_attempts_ram[result_wptr] <= scanner_attempts;
                res_found_ram[result_wptr] <= scanner_found;
                res_nonce_ram[result_wptr] <= scanner_winner_nonce;
                res_hash_ram[result_wptr] <= scanner_winner_hash;
                result_wptr <= step_ptr(result_wptr);
            end
            if (pop_result) result_rptr <= step_ptr(result_rptr);
            case ({push_result, pop_result})
                2'b10: result_used <= result_used + 1'b1;
                2'b01: result_used <= result_used - 1'b1;
                default: ;
            endcase

            case (state)
                IDLE: if (pop_command) begin
                    scanner_start <= 1;
                    state <= WAIT_ARM;
                end
                // Start is sampled on the next edge, so wait one extra
                // clock before inspecting done. Zero-count scans complete
                // immediately, without ever asserting scanner_busy.
                WAIT_ARM: state <= WAIT_DONE;
                WAIT_DONE: if (scanner_done) state <= IDLE;
                default: state <= IDLE;
            endcase
        end
    end
endmodule
