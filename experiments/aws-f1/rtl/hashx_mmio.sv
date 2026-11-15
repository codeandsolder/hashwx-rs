// Independent F1 AppPF BAR0 MMIO bank for HashX v1.
// SPDX-License-Identifier: LGPL-3.0-only
module hashx_mmio(
    input logic clk, rst, wr_en,
    input logic [31:0] wr_addr, wr_data, rd_addr,
    output logic [31:0] rd_value
);
    logic [63:0] first_nonce, target;
    logic [31:0] nonce_count;
    logic scan_start, scan_busy, scan_done, scan_found;
    logic [31:0] attempts;
    logic [63:0] winner_nonce;
    logic [255:0] winner_digest;
    // Config 0x4000-0x501c inclusive, 1032x32-bit words.
    wire cfg_we = wr_en && wr_addr >= 32'h4000
        && wr_addr < 32'h5020 && wr_addr[1:0] == 0;
    hashx_scan scanner(
        .clk(clk), .rst(rst),
        .cfg_we(cfg_we), .cfg_addr(wr_addr[12:2]), .cfg_data(wr_data),
        .start(scan_start), .first_nonce(first_nonce), .target(target),
        .nonce_count(nonce_count),
        .busy(scan_busy), .done(scan_done), .found(scan_found),
        .attempts(attempts), .winner_nonce(winner_nonce), .winner_digest(winner_digest)
    );
    always_ff @(posedge clk) begin
        if (rst) begin
            first_nonce <= 0;
            target <= 0;
            nonce_count <= 0;
            scan_start <= 0;
        end else begin
            scan_start <= 0;
            if (wr_en && !scan_busy) case (wr_addr)
                32'h1004: scan_start <= wr_data[0];
                32'h1008: first_nonce[31:0] <= wr_data;
                32'h100c: first_nonce[63:32] <= wr_data;
                32'h1010: target[31:0] <= wr_data;
                32'h1014: target[63:32] <= wr_data;
                32'h1018: nonce_count <= wr_data;
                default: ;
            endcase
        end
    end
    always_comb begin
        rd_value = 32'hDEADBEEF;
        case (rd_addr)
            32'h1000: rd_value = 32'h48585831; // HXX1
            32'h1008: rd_value = first_nonce[31:0];
            32'h100c: rd_value = first_nonce[63:32];
            32'h1010: rd_value = target[31:0];
            32'h1014: rd_value = target[63:32];
            32'h1018: rd_value = nonce_count;
            32'h101c: rd_value = {29'b0,scan_found,scan_done,scan_busy};
            32'h1020: rd_value = attempts;
            32'h1024: rd_value = winner_nonce[31:0];
            32'h1028: rd_value = winner_nonce[63:32];
            32'h102c: rd_value = winner_digest[31:0];
            32'h1030: rd_value = winner_digest[63:32];
            32'h1034: rd_value = winner_digest[95:64];
            32'h1038: rd_value = winner_digest[127:96];
            32'h103c: rd_value = winner_digest[159:128];
            32'h1040: rd_value = winner_digest[191:160];
            32'h1044: rd_value = winner_digest[223:192];
            32'h1048: rd_value = winner_digest[255:224];
            default: ;
        endcase
    end
endmodule
