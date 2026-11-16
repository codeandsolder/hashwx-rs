// Synthesizable AppPF BAR0 register decoder for both independent F1 engines.
// The same module is instantiated by AWS shell and exercised by simulation.
// SPDX-License-Identifier: LGPL-3.0-only
module hash_f1_dual #(
    parameter integer WX_LANES = 4,
    parameter integer HX_LANES = 4,
    parameter integer HX_MUL_PARTIALS = 4,
    parameter bit HX_DIRECT_MUL = 1'b1
) (
    input logic clk, rst, wr_en,
    input logic [31:0] wr_addr, wr_data, rd_addr,
    output logic [31:0] rd_value,
    output logic [15:0] pcim_awid, pcim_arid,
    output logic [63:0] pcim_awaddr, pcim_araddr,
    output logic [7:0] pcim_awlen, pcim_arlen,
    output logic [2:0] pcim_awsize, pcim_arsize,
    output logic pcim_awvalid, pcim_arvalid,
    input logic pcim_awready, pcim_arready,
    output logic [511:0] pcim_wdata,
    output logic [63:0] pcim_wstrb,
    output logic pcim_wlast, pcim_wvalid,
    input logic pcim_wready,
    input logic [15:0] pcim_bid, pcim_rid,
    input logic [1:0] pcim_bresp, pcim_rresp,
    input logic pcim_bvalid, pcim_rvalid, pcim_rlast,
    input logic [511:0] pcim_rdata,
    output logic pcim_bready, pcim_rready
);
    logic [31:0] hashwx_read_data, hashx_read_data, dma_read_data;
    logic [1:0] dma_cmd_valid, dma_cmd_ready, dma_result_pop;
    logic [1:0] dma_result_valid, dma_result_found, bank_idle;
    logic dma_active;
    logic [63:0] dma_cmd_nonce, dma_cmd_target;
    logic [31:0] dma_cmd_count, dma_cmd_tag;
    logic [31:0] wx_tag, hx_tag, wx_attempts, hx_attempts;
    logic [63:0] wx_nonce, hx_nonce, wx_hash;
    logic [255:0] hx_hash;

    hashwx_mmio #(.LANES(WX_LANES)) wx (
        .clk(clk), .rst(rst), .wr_en(wr_en),
        .wr_addr(wr_addr), .wr_data(wr_data),
        .rd_addr(rd_addr), .rd_value(hashwx_read_data),
        .dma_active(dma_active), .dma_command_valid(dma_cmd_valid[0]),
        .dma_command_ready(dma_cmd_ready[0]),
        .dma_command_nonce(dma_cmd_nonce), .dma_command_target(dma_cmd_target),
        .dma_command_count(dma_cmd_count), .dma_command_tag(dma_cmd_tag),
        .dma_result_pop(dma_result_pop[0]),
        .dma_result_valid(dma_result_valid[0]),
        .dma_result_found(dma_result_found[0]),
        .dma_result_tag(wx_tag), .dma_result_attempts(wx_attempts),
        .dma_result_nonce(wx_nonce), .dma_result_hash(wx_hash),
        .dma_config_idle(bank_idle[0])
    );
    hashx_mmio #(.LANES(HX_LANES), .MUL_PARTIALS(HX_MUL_PARTIALS),
                  .DIRECT_MUL(HX_DIRECT_MUL)) hx (
        .clk(clk), .rst(rst), .wr_en(wr_en),
        .wr_addr(wr_addr), .wr_data(wr_data),
        .rd_addr(rd_addr), .rd_value(hashx_read_data),
        .dma_active(dma_active), .dma_command_valid(dma_cmd_valid[1]),
        .dma_command_ready(dma_cmd_ready[1]),
        .dma_command_nonce(dma_cmd_nonce), .dma_command_target(dma_cmd_target),
        .dma_command_count(dma_cmd_count), .dma_command_tag(dma_cmd_tag),
        .dma_result_pop(dma_result_pop[1]),
        .dma_result_valid(dma_result_valid[1]),
        .dma_result_found(dma_result_found[1]),
        .dma_result_tag(hx_tag), .dma_result_attempts(hx_attempts),
        .dma_result_nonce(hx_nonce), .dma_result_hash(hx_hash),
        .dma_config_idle(bank_idle[1])
    );

    hash_pcim_dma dma (
        .clk(clk), .rst(rst),
        .mmio_we(wr_en), .mmio_addr(wr_addr), .mmio_data(wr_data),
        .mmio_rd_addr(rd_addr), .mmio_rd_data(dma_read_data),
        .active(dma_active), .bank_idle(bank_idle),
        .cmd_valid(dma_cmd_valid), .cmd_ready(dma_cmd_ready),
        .cmd_nonce(dma_cmd_nonce), .cmd_target(dma_cmd_target),
        .cmd_count(dma_cmd_count), .cmd_tag(dma_cmd_tag),
        .result_valid(dma_result_valid), .result_found(dma_result_found),
        .result_pop(dma_result_pop),
        .wx_tag(wx_tag), .hx_tag(hx_tag),
        .wx_attempts(wx_attempts), .hx_attempts(hx_attempts),
        .wx_nonce(wx_nonce), .hx_nonce(hx_nonce),
        .wx_hash(wx_hash), .hx_hash(hx_hash),
        .awid(pcim_awid), .awaddr(pcim_awaddr), .awlen(pcim_awlen),
        .awsize(pcim_awsize), .awvalid(pcim_awvalid), .awready(pcim_awready),
        .wdata(pcim_wdata), .wstrb(pcim_wstrb), .wlast(pcim_wlast),
        .wvalid(pcim_wvalid), .wready(pcim_wready),
        .bid(pcim_bid), .bresp(pcim_bresp), .bvalid(pcim_bvalid),
        .bready(pcim_bready),
        .arid(pcim_arid), .araddr(pcim_araddr), .arlen(pcim_arlen),
        .arsize(pcim_arsize), .arvalid(pcim_arvalid), .arready(pcim_arready),
        .rid(pcim_rid), .rdata(pcim_rdata), .rresp(pcim_rresp),
        .rlast(pcim_rlast), .rvalid(pcim_rvalid), .rready(pcim_rready)
    );

    always_comb begin
        if (rd_addr >= 32'h2000 && rd_addr < 32'h2040)
            rd_value = dma_read_data;
        else if (rd_addr >= 32'h1000 && rd_addr < 32'h1068)
            rd_value = hashx_read_data;
        else
            rd_value = hashwx_read_data;
    end
endmodule
