// Integration model: F1 OCL BAR0 address fanout with both HashX and HashWX.
module tb_f1_dual;
    logic clk=0, rst=1, wr_en=0;
    logic [31:0] wr_addr=0,wr_data=0,rd_addr=0;
    wire [31:0] combined_read;
    logic [31:0] wx_cfg[0:131],hx_cfg[0:1031];
    logic [63:0] wx_golden[0:2];
    logic [255:0] hx_golden[0:2];
    hash_f1_dual dut(
        .clk(clk),.rst(rst),.wr_en(wr_en),
        .wr_addr(wr_addr),.wr_data(wr_data),.rd_addr(rd_addr),
        .rd_value(combined_read)
    );
    always #5 clk=~clk;
    task automatic poke(input logic [31:0] addr,input logic [31:0] val);
        @(negedge clk);
        wr_en=1;wr_addr=addr;wr_data=val;
        @(negedge clk);
        wr_en=0;
    endtask
    task automatic peek(input logic [31:0] addr,input logic [31:0] expected);
        rd_addr=addr;
        #1;
        if (combined_read !== expected)
            $fatal(1,"dual read %x expected %x got %x",addr,expected,combined_read);
    endtask
    initial begin
        $readmemh("tests/fixtures/config.hex",wx_cfg);
        $readmemh("tests/fixtures/golden.hex",wx_golden);
        $readmemh("tests/fixtures/hashx_config_00.hex",hx_cfg);
        $readmemh("tests/fixtures/hashx_golden_00.hex",hx_golden);
        repeat(3) @(negedge clk);
        rst=0;
        peek(32'h600,32'h48585731);
        peek(32'h1000,32'h48585831);
        for (int i=0;i<132;i++) poke(32'h800+i*4,wx_cfg[i]);
        for (int i=0;i<1032;i++) poke(32'h4000+i*4,hx_cfg[i]);
        // Same requested nonce and a guaranteed early win for both.
        poke(32'h608,0);
        poke(32'h60c,0);
        poke(32'h610,wx_golden[0][31:0]+1'b1);
        poke(32'h614,wx_golden[0][63:32]);
        poke(32'h618,1);
        poke(32'h1008,0);
        poke(32'h100c,0);
        poke(32'h1010,hx_golden[0][31:0]+1'b1);
        poke(32'h1014,hx_golden[0][63:32]);
        poke(32'h1018,1);
        poke(32'h604,1);
        poke(32'h1004,1);
        begin: both_done
            for (int i=0;i<30000;i++) begin
                @(negedge clk);
                if(dut.wx.scan_done && dut.hx.scan_done) disable both_done;
            end
            $fatal(1,"dual concurrent scan timed out");
        end
        peek(32'h61c,6);
        peek(32'h101c,6);
        peek(32'h620,1);
        peek(32'h1020,1);
        peek(32'h62c,wx_golden[0][31:0]);
        peek(32'h630,wx_golden[0][63:32]);
        for(int i=0;i<8;i++) peek(32'h102c+i*4,hx_golden[0][i*32 +: 32]);
        $display("PASS dual independent HashX/HashWX F1 BAR0 dispatch with concurrent scans");
        $finish;
    end
endmodule
