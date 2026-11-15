module tb_hashx_mmio;
    logic clk=0, rst=1, wr_en=0;
    logic [31:0] wr_addr=0,wr_data=0,rd_addr=0;
    wire [31:0] rd_value;
    logic [31:0] cfg[0:1031];
    logic [255:0] gold[0:2];
    hashx_mmio dut(.*);
    always #5 clk=~clk;
    task automatic poke(input logic[31:0] addr, input logic[31:0] value);
        @(negedge clk);
        wr_en=1; wr_addr=addr; wr_data=value;
        @(negedge clk);
        wr_en=0;
    endtask
    task automatic peek(input logic[31:0] addr, input logic[31:0] expected);
        rd_addr=addr; #1;
        if (rd_value !== expected)
            $fatal(1, "HashX MMIO 0x%x expected 0x%x actual 0x%x",addr,expected,rd_value);
    endtask
    initial begin
        $readmemh("tests/fixtures/hashx_config_00.hex",cfg);
        $readmemh("tests/fixtures/hashx_golden_00.hex",gold);
        repeat(3) @(negedge clk);
        rst=0;
        peek(32'h1000,32'h48585832);
        for (int i=0;i<1032;i++) poke(32'h4000+i*4,cfg[i]);
        poke(32'h1008,0);
        poke(32'h100c,0);
        poke(32'h1010,gold[0][31:0]+1'b1);
        poke(32'h1014,gold[0][63:32]);
        poke(32'h1018,3);
        poke(32'h1004,1);
        begin : wait_for_scan
            for(int i=0;i<50000;i++) begin
                @(negedge clk);
                rd_addr=32'h101c; #1;
                if (rd_value[1]) disable wait_for_scan;
            end
            $fatal(1,"HashX MMIO scan timeout");
        end
        peek(32'h101c,32'hf);
        peek(32'h1020,32'h3);
        peek(32'h1024,0);
        peek(32'h1028,0);
        for(int i=0;i<8;i++) peek(32'h102c+i*4,gold[0][i*32 +: 32]);
        peek(32'h1050,0);
        peek(32'h1058,32'h00010000);
        poke(32'h1054,1);
        peek(32'h101c,32'h8);
        $display("PASS HashX F1 OCL v2 queue: program, scan, full 256-bit digest, pop");
        $finish;
    end
endmodule
