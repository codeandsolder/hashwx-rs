// Synthesizable AppPF BAR0 register decoder for both independent F1 engines.
// The same module is instantiated by AWS shell and exercised by simulation.
// SPDX-License-Identifier: LGPL-3.0-only
module hash_f1_dual #(
    parameter integer WX_LANES = 4,
    parameter integer HX_LANES = 4
) (
    input logic clk, rst, wr_en,
    input logic [31:0] wr_addr, wr_data, rd_addr,
    output logic [31:0] rd_value
);
    logic [31:0] hashwx_read_data, hashx_read_data;

    hashwx_mmio #(.LANES(WX_LANES)) wx (
        .clk(clk), .rst(rst), .wr_en(wr_en),
        .wr_addr(wr_addr), .wr_data(wr_data),
        .rd_addr(rd_addr), .rd_value(hashwx_read_data)
    );
    hashx_mmio #(.LANES(HX_LANES)) hx (
        .clk(clk), .rst(rst), .wr_en(wr_en),
        .wr_addr(wr_addr), .wr_data(wr_data),
        .rd_addr(rd_addr), .rd_value(hashx_read_data)
    );

    always_comb begin
        if (rd_addr >= 32'h1000 && rd_addr < 32'h1068)
            rd_value = hashx_read_data;
        else
            rd_value = hashwx_read_data;
    end
endmodule
