module tb_hashwx_mmio;
    logic clk=0, rst=1, wr_en=0;
    logic [31:0] wr_addr=0,wr_data=0,rd_addr=0;
    wire [31:0] rd_value;
    logic [31:0] cfg [0:131];
    logic [63:0] golden [0:2];
    hashwx_mmio dut(
        .clk(clk), .rst(rst), .wr_en(wr_en),
        .wr_addr(wr_addr), .wr_data(wr_data), .rd_addr(rd_addr),
        .rd_value(rd_value),
        .dma_active(1'b0), .dma_command_valid(1'b0),
        .dma_result_pop(1'b0),
        .dma_command_nonce('0), .dma_command_target('0),
        .dma_command_count('0), .dma_command_tag('0),
        .dma_command_ready(), .dma_result_valid(), .dma_result_found(),
        .dma_result_tag(), .dma_result_attempts(), .dma_result_nonce(),
        .dma_result_hash(), .dma_config_idle()
    );
    always #5 clk = ~clk;
    task automatic poke(input logic [31:0] addr, input logic [31:0] val);
        @(negedge clk);
        wr_en = 1;
        wr_addr = addr;
        wr_data = val;
        @(negedge clk);
        wr_en = 0;
    endtask
    task automatic peek(input logic [31:0] addr, input logic [31:0] val);
        rd_addr = addr;
        #1;
        if (rd_value !== val)
            $fatal(1,"peek addr=%x expected=%x got=%x",addr,val,rd_value);
    endtask
    initial begin
        $readmemh("tests/fixtures/config.hex",cfg);
        $readmemh("tests/fixtures/golden.hex",golden);
        repeat(3) @(negedge clk);
        rst=0;
        peek(32'h600,32'h48585732);
        for(int i=0;i<132;i++) poke(32'h800+i*4,cfg[i]);
        poke(32'h608,0);
        poke(32'h60C,0);
        poke(32'h610,golden[0][31:0]+1);
        // Lower half overflow is not present in this particular golden hash.
        poke(32'h614,golden[0][63:32]);
        poke(32'h618,3);
        peek(32'h618,3);
        poke(32'h604,1);
        begin: wait_for_scan
            for(int i=0;i<150000;i++) begin
                @(negedge clk);
                rd_addr=32'h61c;
                #1;
                if(rd_value[1]) disable wait_for_scan;
            end
            $fatal(1,"MMIO scan timeout");
        end
        peek(32'h61C,32'd15); // done + found, no busy
        peek(32'h620,32'd3);
        peek(32'h624,0);
        peek(32'h628,0);
        peek(32'h62C,golden[0][31:0]);
        peek(32'h630,golden[0][63:32]);
        peek(32'h638,0);
        peek(32'h640,32'h00010000);
        poke(32'h63c,1);
        peek(32'h61c,32'd8);
        $display("PASS F1 OCL v2 queue loading, scan, completion, pop");
        $finish;
    end
endmodule
