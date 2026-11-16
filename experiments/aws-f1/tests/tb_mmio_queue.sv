// End-to-end queued MMIO tests, including concurrent independent algorithms,
// blocked program mutation and ordered full-width completion readback.
module tb_mmio_queue;
    logic clk=0,rst=1,wr_en=0;
    logic [31:0] wr_addr=0,wr_data=0,rd_addr=0;
    wire [31:0] rd_value;
    logic [31:0] wx_cfg[0:131],hx_cfg[0:1032];
    logic [63:0] wx_gold[0:2];
    logic [255:0] hx_gold[0:2];
    hash_f1_dual dut(
        .clk(clk), .rst(rst), .wr_en(wr_en),
        .wr_addr(wr_addr), .wr_data(wr_data), .rd_addr(rd_addr),
        .rd_value(rd_value)
    );
    always #5 clk=~clk;

    task automatic poke(input logic [31:0] addr, val);
        @(negedge clk);
        wr_en=1;wr_addr=addr;wr_data=val;
        @(negedge clk);
        wr_en=0;
    endtask
    task automatic peek(input logic [31:0] addr, expected);
        rd_addr=addr;#1;
        if(rd_value!==expected)
            $fatal(1,"queue reg=0x%h expected=0x%h actual=0x%h",
                addr,expected,rd_value);
    endtask
    task automatic job(
        input logic [31:0] start_reg, input logic [31:0] base,
        input logic [31:0] tag_reg, input logic [31:0] tag,
        input logic [63:0] nonce, threshold,
        input logic [31:0] count
    );
        poke(base,nonce[31:0]);poke(base+4,nonce[63:32]);
        poke(base+8,threshold[31:0]);poke(base+12,threshold[63:32]);
        poke(base+16,count);poke(tag_reg,tag);
        poke(start_reg,1);
    endtask
    task automatic wait_queued_results;
        integer cycles;
        cycles=0;
        while(cycles<120000) begin
            @(negedge clk);
            if(dut.wx.queued_results==3 && dut.hx.queued_results==3)
                return;
            cycles++;
        end
        $fatal(1,"queue completion timeout WX=%d HX=%d",
            dut.wx.queued_results,dut.hx.queued_results);
    endtask
    task automatic check_wx(input logic [31:0] tag, attempts,
                            input logic found,
                            input logic [63:0] nonce, digest);
        peek(32'h638,tag);
        peek(32'h620,attempts);
        if(found) begin
            peek(32'h624,nonce[31:0]);peek(32'h628,nonce[63:32]);
            peek(32'h62c,digest[31:0]);peek(32'h630,digest[63:32]);
        end
        if(dut.wx.completion_found!==found)
            $fatal(1,"WX found mismatch tag=%0d",tag);
        poke(32'h63c,1);
    endtask
    task automatic check_hx(input logic [31:0] tag, attempts,
                            input logic found,
                            input logic [63:0] nonce,
                            input logic [255:0] digest);
        peek(32'h1050,tag);
        peek(32'h1020,attempts);
        if(found) begin
            peek(32'h1024,nonce[31:0]);peek(32'h1028,nonce[63:32]);
            for(integer k=0;k<8;k++)
                peek(32'h102c+32'(k*4),digest[k*32 +: 32]);
        end
        if(dut.hx.completion_found!==found)
            $fatal(1,"HX found mismatch tag=%0d",tag);
        poke(32'h1054,1);
    endtask

    initial begin
        $readmemh("tests/fixtures/config.hex",wx_cfg);
        $readmemh("tests/fixtures/golden.hex",wx_gold);
        $readmemh("tests/fixtures/hashx_config_00.hex",hx_cfg);
        $readmemh("tests/fixtures/hashx_golden_00.hex",hx_gold);
        repeat(3) @(negedge clk);rst=0;
        peek(32'h600,32'h48585732);peek(32'h1000,32'h48585832);
        peek(32'h648,4);peek(32'h1060,4);
        for(integer i=0;i<132;i++)poke(32'h800+32'(i*4),wx_cfg[i]);
        for(integer i=0;i<1033;i++)poke(32'h4000+32'(i*4),hx_cfg[i]);

        // Submit three jobs per bank before waiting for any result. The
        // staged registers must be snapshotted per enqueue.
        job(32'h604,32'h608,32'h644,10,0,0,2); // no hit
        job(32'h1004,32'h1008,32'h105c,20,0,0,2);
        job(32'h604,32'h608,32'h644,11,1,wx_gold[1]+1'b1,1);
        job(32'h1004,32'h1008,32'h105c,21,1,hx_gold[1][63:0]+1'b1,1);
        job(32'h604,32'h608,32'h644,12,0,0,0);
        job(32'h1004,32'h1008,32'h105c,22,0,0,0);
        // Program writes while commands exist must be rejected, not
        // silently change only the idle lanes' microcode.
        poke(32'h800,32'hbadbeef);
        poke(32'h4000,32'hbadbeef);
        peek(32'h64c,2);
        peek(32'h1064,2);
        wait_queued_results();

        peek(32'h640,32'h00030000);
        peek(32'h1058,32'h00030000);
        check_wx(10,2,0,0,0);
        check_hx(20,2,0,0,0);
        check_wx(11,1,1,1,wx_gold[1]);
        check_hx(21,1,1,1,hx_gold[1]);
        check_wx(12,0,0,0,0);
        check_hx(22,0,0,0,0);
        peek(32'h640,0);
        peek(32'h1058,0);
        poke(32'h64c,2);
        poke(32'h1064,2);
        peek(32'h61c,8);
        peek(32'h101c,8);
        peek(32'h64c,0);
        peek(32'h1064,0);
        $display("PASS F1 v2 MMIO: six queued jobs, independent FIFO order, tags, no-hit/winner/empty, protected program");
        $finish;
    end
endmodule
