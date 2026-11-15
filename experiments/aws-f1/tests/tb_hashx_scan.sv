// HashX bounded search regression: exclusive target, no hit, zero work,
// second nonce winner, and first-hit path.
module tb_hashx_scan;
    logic clk=0, rst=1, cfg_we=0, start=0;
    logic [10:0] cfg_addr=0;
    logic [31:0] cfg_data=0;
    logic [63:0] first_nonce=0, target=0;
    logic [31:0] nonce_count=0;
    wire busy, done, found;
    wire [31:0] attempts;
    wire [63:0] winner_nonce;
    wire [255:0] winner_digest;
    logic [31:0] cfg [0:1031];
    logic [255:0] golden [0:2];
    hashx_scan dut(.*);
    always #5 clk=~clk;
    task automatic write_cfg(input integer addr, input logic [31:0] value);
        @(negedge clk);
        cfg_we = 1;
        cfg_addr = addr[10:0];
        cfg_data = value;
        @(negedge clk);
        cfg_we = 0;
    endtask
    task automatic check_scan(
        input logic [31:0] count,
        input logic [63:0] threshold,
        input logic expected_found,
        input logic [63:0] expected_nonce,
        input logic [31:0] expected_attempts
    );
        integer cycles;
        @(negedge clk);
        first_nonce = 0;
        nonce_count = count;
        target = threshold;
        start = 1;
        @(negedge clk);
        start = 0;
        cycles = 0;
        while (!done && cycles < 120000) begin
            @(negedge clk);
            cycles++;
        end
        if (!done) $fatal(1,"HashX scan timeout count=%d",count);
        if (found !== expected_found || attempts !== expected_attempts)
            $fatal(1,"HashX scan count=%d target=%h expected found=%b attempts=%0d got %b %0d",
                count,threshold,expected_found,expected_attempts,found,attempts);
        if (expected_found) begin
            if (winner_nonce !== expected_nonce ||
                winner_digest !== golden[expected_nonce[1:0]])
                $fatal(1,"HashX scan wrong winner nonce=%h digest=%h",
                    winner_nonce,winner_digest);
        end
        $display("PASS HashX scan count=%0d target=%h attempts=%0d found=%b nonce=%h",
            count,threshold,attempts,found,winner_nonce);
    endtask
    initial begin
        $readmemh("tests/fixtures/hashx_config_00.hex",cfg);
        $readmemh("tests/fixtures/hashx_golden_00.hex",golden);
        repeat(3) @(negedge clk);
        rst=0;
        for (int i=0;i<1032;i++) write_cfg(i,cfg[i]);
        if (!(golden[1][63:0] < golden[0][63:0]))
            $fatal(1, "invalid differential fixture: nonce1 must hash below nonce0");
        check_scan(3,0,0,0,3);
        check_scan(0,0,0,0,0);
        check_scan(1,golden[0][63:0],0,0,1);
        check_scan(2,golden[0][63:0],1,1,2);
        check_scan(3,golden[0][63:0]+1'b1,1,0,3);
        $display("PASS HashX scan boundaries, empty, no-hit, second-hit and first-hit");
        $finish;
    end
endmodule
