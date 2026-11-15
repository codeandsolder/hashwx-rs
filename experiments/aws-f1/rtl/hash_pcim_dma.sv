// F1 host-memory DMA: one 64-byte AXI4 PCIM transfer per descriptor/result.
// Host controls monotonic producer and acknowledgment counters over OCL.
// The physical rings reside in pinned, IOMMU-mapped HOST memory.
// SPDX-License-Identifier: LGPL-3.0-only
module hash_pcim_dma #(
    parameter integer RING_LOG2 = 4
)(
    input logic clk, rst, mmio_we,
    input logic [31:0] mmio_addr, mmio_data, mmio_rd_addr,
    output logic [31:0] mmio_rd_data,
    output logic active,
    input logic [1:0] bank_idle,
    output logic [1:0] cmd_valid,
    input logic [1:0] cmd_ready,
    output logic [63:0] cmd_nonce, cmd_target,
    output logic [31:0] cmd_count, cmd_tag,
    input logic [1:0] result_valid, result_found,
    output logic [1:0] result_pop,
    input logic [31:0] wx_tag, hx_tag, wx_attempts, hx_attempts,
    input logic [63:0] wx_nonce, hx_nonce, wx_hash,
    input logic [255:0] hx_hash,

    output logic [15:0] awid, arid,
    output logic [63:0] awaddr, araddr,
    output logic [7:0] awlen, arlen,
    output logic [2:0] awsize, arsize,
    output logic awvalid, arvalid,
    input logic awready, arready,
    output logic [511:0] wdata,
    output logic [63:0] wstrb,
    output logic wlast, wvalid,
    input logic wready,
    input logic [15:0] bid, rid,
    input logic [1:0] bresp, rresp,
    input logic bvalid, rvalid, rlast,
    input logic [511:0] rdata,
    output logic bready, rready
);
    localparam logic [31:0] RING_ENTRIES = 32'(1 << RING_LOG2);
    localparam logic [63:0] RING_BYTES = 64'(RING_ENTRIES) << 6;
    typedef enum logic [3:0] {
        STOPPED, IDLE, READ_ADDR, READ_DATA, DISPATCH,
        WRITE_ADDR, WRITE_DATA, WRITE_RESP, FAILED
    } state_t;
    state_t state;
    logic [63:0] descriptor_base, completion_base;
    logic [31:0] host_producer, host_ack, descriptor_consumed, completion_produced;
    logic [31:0] fault;
    logic [511:0] descriptor, completion;
    logic write_hx, prefer_hx;
    logic [31:0] desc_pending, result_outstanding;
    wire in_range = mmio_addr >= 32'h2000 && mmio_addr < 32'h2040;
    // Reject partially overlapping rings and 64-bit wrap on the last beat.
    wire ring_addresses_valid = descriptor_base >= 64 &&
        completion_base >= 64 &&
        descriptor_base <= (64'hffffffffffffffff - RING_BYTES + 1) &&
        completion_base <= (64'hffffffffffffffff - RING_BYTES + 1) &&
        (descriptor_base + RING_BYTES <= completion_base ||
         completion_base + RING_BYTES <= descriptor_base);

    assign active = state != STOPPED;
    assign desc_pending = host_producer - descriptor_consumed;
    assign result_outstanding = completion_produced - host_ack;
    assign cmd_nonce = descriptor[191:128];
    assign cmd_target = descriptor[255:192];
    assign cmd_count = descriptor[95:64];
    assign cmd_tag = descriptor[31:0];
    assign cmd_valid[0] = state == DISPATCH && descriptor[63:32] == 0;
    assign cmd_valid[1] = state == DISPATCH && descriptor[63:32] == 1;
    assign result_pop[0] = state == WRITE_RESP && bvalid && bresp == 0
        && bid == 0 && !write_hx;
    assign result_pop[1] = state == WRITE_RESP && bvalid && bresp == 0
        && bid == 0 && write_hx;

    // Single-beat 512-bit AXI4, 64B aligned; fixed 64B beats required by
    // the pinned F1 shell's PCIM port.
    assign awid = 0;
    assign arid = 0;
    assign awlen = 0;
    assign arlen = 0;
    assign awsize = 3'd6;
    assign arsize = 3'd6;
    assign awaddr = completion_base
        + (64'(completion_produced & 32'(RING_ENTRIES-1)) << 6);
    assign araddr = descriptor_base
        + (64'(descriptor_consumed & 32'(RING_ENTRIES-1)) << 6);
    assign awvalid = state == WRITE_ADDR;
    assign arvalid = state == READ_ADDR;
    assign wdata = completion;
    assign wstrb = {64{1'b1}};
    assign wlast = 1;
    assign wvalid = state == WRITE_DATA;
    assign bready = state == WRITE_RESP;
    assign rready = state == READ_DATA;

    always_comb begin
        mmio_rd_data = 32'hdeadc0de;
        case (mmio_rd_addr)
            32'h2000: mmio_rd_data = 32'(active);
            32'h2004: mmio_rd_data = 32'h5043494d; // PCIM
            32'h2008: mmio_rd_data = descriptor_base[31:0];
            32'h200c: mmio_rd_data = descriptor_base[63:32];
            32'h2010: mmio_rd_data = completion_base[31:0];
            32'h2014: mmio_rd_data = completion_base[63:32];
            32'h2018: mmio_rd_data = host_producer;
            32'h201c: mmio_rd_data = host_ack;
            32'h2020: mmio_rd_data = descriptor_consumed;
            32'h2024: mmio_rd_data = completion_produced;
            32'h2028: mmio_rd_data = {23'd0, state, 4'(prefer_hx), 1'(fault != 0)};
            32'h202c: mmio_rd_data = fault;
            32'h2030: mmio_rd_data = RING_ENTRIES;
            default: ;
        endcase
    end

    // Completion record: tag, algorithm, attempts, found, winner nonce,
    // reserved 64-bit zero, then full 256-bit digest (HashWX zero-extends).
    always_comb begin
        completion = '0;
        completion[31:0] = write_hx ? hx_tag : wx_tag;
        completion[63:32] = 32'(write_hx);
        completion[95:64] = write_hx ? hx_attempts : wx_attempts;
        completion[96] = write_hx ? result_found[1] : result_found[0];
        completion[191:128] = write_hx ? hx_nonce : wx_nonce;
        completion[511:256] = write_hx ? hx_hash : {192'd0, wx_hash};
    end

    always_ff @(posedge clk) begin
        if (rst) begin
            state <= STOPPED;
            descriptor_base <= 0;
            completion_base <= 0;
            host_producer <= 0;
            host_ack <= 0;
            descriptor_consumed <= 0;
            completion_produced <= 0;
            fault <= 0;
            descriptor <= 0;
            write_hx <= 0;
            prefer_hx <= 0;
        end else begin
            if (mmio_we && in_range) begin
                case (mmio_addr)
                    32'h2000: if (mmio_data[0]) begin
                        if (state == STOPPED && descriptor_base[5:0] == 0
                            && completion_base[5:0] == 0
                            && descriptor_base != 0 && completion_base != 0
                            && ring_addresses_valid
                            && bank_idle == 2'b11
                            && host_producer == 0 && host_ack == 0) begin
                            descriptor_consumed <= 0;
                            completion_produced <= 0;
                            fault <= 0;
                            prefer_hx <= 0;
                            state <= IDLE;
                        end else fault <= 32'h1;
                    end else if (state == IDLE && desc_pending == 0
                        && result_outstanding == 0 && bank_idle == 2'b11) begin
                        state <= STOPPED;
                        host_producer <= 0;
                        host_ack <= 0;
                        descriptor_consumed <= 0;
                        completion_produced <= 0;
                    end else fault <= 32'h2;
                    32'h2008: if (state == STOPPED)
                        descriptor_base[31:0] <= mmio_data;
                    32'h200c: if (state == STOPPED)
                        descriptor_base[63:32] <= mmio_data;
                    32'h2010: if (state == STOPPED)
                        completion_base[31:0] <= mmio_data;
                    32'h2014: if (state == STOPPED)
                        completion_base[63:32] <= mmio_data;
                    32'h2018: if (state == IDLE || state == READ_ADDR
                            || state == READ_DATA || state == DISPATCH
                            || state == WRITE_ADDR || state == WRITE_DATA
                            || state == WRITE_RESP) host_producer <= mmio_data;
                    32'h201c: if (active && state != FAILED) host_ack <= mmio_data;
                    default: ;
                endcase
            end
            case (state)
                IDLE: begin
                    if (desc_pending > RING_ENTRIES ||
                        result_outstanding > RING_ENTRIES) begin
                        fault <= 32'h3;
                        state <= FAILED;
                    end else if (result_outstanding < RING_ENTRIES
                        && (result_valid[prefer_hx] || result_valid[!prefer_hx])) begin
                        write_hx <= result_valid[prefer_hx] ? prefer_hx : !prefer_hx;
                        state <= WRITE_ADDR;
                    end else if (desc_pending != 0) state <= READ_ADDR;
                end
                READ_ADDR: if (arready) state <= READ_DATA;
                READ_DATA: if (rvalid) begin
                    if (rresp != 0 || !rlast || rid != 0) begin
                        fault <= 32'h4; state <= FAILED;
                    end else begin
                        descriptor <= rdata;
                        state <= DISPATCH;
                    end
                end
                DISPATCH: begin
                    if (descriptor[63:32] > 1 || descriptor[127:96] != 0
                        || descriptor[511:256] != 0) begin
                        fault <= 32'h5;
                        state <= FAILED;
                    end else if ((cmd_valid[0] && cmd_ready[0])
                        || (cmd_valid[1] && cmd_ready[1])) begin
                        descriptor_consumed <= descriptor_consumed + 1'b1;
                        state <= IDLE;
                    end
                end
                WRITE_ADDR: if (awready) state <= WRITE_DATA;
                WRITE_DATA: if (wready) state <= WRITE_RESP;
                WRITE_RESP: if (bvalid) begin
                    if (bresp != 0 || bid != 0) begin
                        fault <= 32'h6; state <= FAILED;
                    end else begin
                        completion_produced <= completion_produced + 1'b1;
                        prefer_hx <= !write_hx;
                        state <= IDLE;
                    end
                end
                default: ;
            endcase
        end
    end
endmodule
