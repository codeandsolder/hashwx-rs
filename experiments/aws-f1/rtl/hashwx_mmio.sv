// OCL 32-bit MMIO endpoint. AXI-Lite handshake is owned by AWS's F1 shell CL.
// SPDX-License-Identifier: LGPL-3.0-only
module hashwx_mmio (
    input  logic clk, rst, wr_en,
    input  logic [31:0] wr_addr, wr_data, rd_addr,
    output logic [31:0] rd_value
);
    logic [63:0] first_nonce, target;
    logic [31:0] nonce_count;
    logic scan_start, scan_busy, scan_done, scan_found;
    logic [31:0] attempts;
    logic [63:0] winner_nonce, winner_hash;
    wire cfg_we = wr_en && wr_addr >= 32'h800
        && wr_addr < 32'hA10 && (wr_addr[1:0] == 0);
    hashwx_scan scanner(
        .clk(clk), .rst(rst),
        .cfg_we(cfg_we), .cfg_addr(wr_addr[9:2]), .cfg_data(wr_data),
        .start(scan_start), .first_nonce(first_nonce), .target(target),
        .nonce_count(nonce_count), .busy(scan_busy), .done(scan_done),
        .found(scan_found), .attempts(attempts),
        .winner_nonce(winner_nonce), .winner_hash(winner_hash)
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
                32'h604: scan_start <= wr_data[0];
                32'h608: first_nonce[31:0] <= wr_data;
                32'h60C: first_nonce[63:32] <= wr_data;
                32'h610: target[31:0] <= wr_data;
                32'h614: target[63:32] <= wr_data;
                32'h618: nonce_count <= wr_data;
                default: ;
            endcase
        end
    end
    always_comb begin
        rd_value = 32'hDEADBEEF;
        case (rd_addr)
            32'h600: rd_value = 32'h48585731; // HXW1
            32'h608: rd_value = first_nonce[31:0];
            32'h60C: rd_value = first_nonce[63:32];
            32'h610: rd_value = target[31:0];
            32'h614: rd_value = target[63:32];
            32'h618: rd_value = nonce_count;
            32'h61C: rd_value = {29'b0,scan_found,scan_done,scan_busy};
            32'h620: rd_value = attempts;
            32'h624: rd_value = winner_nonce[31:0];
            32'h628: rd_value = winner_nonce[63:32];
            32'h62C: rd_value = winner_hash[31:0];
            32'h630: rd_value = winner_hash[63:32];
            default: ;
        endcase
    end
endmodule
