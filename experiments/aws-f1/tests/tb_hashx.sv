module tb_hashx #(
    parameter integer MUL_PARTIALS = 4,
    parameter bit DIRECT_MUL = 1'b1,
    parameter bit DUAL_ISSUE = 1'b1
);
    logic clk=0, rst=1, cfg_we=0, start=0;
    logic [10:0] cfg_addr=0;
    logic [31:0] cfg_data=0;
    logic [63:0] nonce=0, target=0;
    wire busy, done, hit;
    wire [255:0] digest;
    logic [31:0] cfg [0:1031];
    logic [255:0] golden [0:2];
    hashx_core #(.MUL_PARTIALS(MUL_PARTIALS), .DIRECT_MUL(DIRECT_MUL), .DUAL_ISSUE(DUAL_ISSUE)) dut(.*);
    always #5 clk=~clk;
    task automatic write_cfg(input integer addr, input logic [31:0] value);
        @(negedge clk);
        cfg_we=1;
        cfg_addr=addr[10:0];
        cfg_data=value;
        @(negedge clk);
        cfg_we=0;
    endtask
    task automatic check(input logic [63:0] candidate, input logic [255:0] expected);
        integer cycles;
        @(negedge clk);
        nonce=candidate;
        target=expected[63:0]+1'b1;
        start=1;
        @(negedge clk);
        start=0;
        cycles=0;
        while(!done && cycles<100000) begin
            @(negedge clk); cycles++;
        end
        if(!done) $fatal(1,"timeout HashX nonce=%h",candidate);
        if(digest !== expected)
            $fatal(1,"HashX wrong nonce=%h expected=%h actual=%h",candidate,expected,digest);
        if(!hit) $fatal(1,"HashX target check failed nonce=%h",candidate);
        $display("PASS HashX nonce=%016h digest=%h cycles=%0d",candidate,digest,cycles);
    endtask
    initial begin
        repeat(3) @(negedge clk);
        rst=0;
        for(int seed_i=0;seed_i<36;seed_i++) begin
            $readmemh($sformatf("tests/fixtures/hashx_config_%02d.hex",seed_i),cfg);
            $readmemh($sformatf("tests/fixtures/hashx_golden_%02d.hex",seed_i),golden);
            for(int i=0;i<1032;i++) write_cfg(i,cfg[i]);
            check(0,golden[0]);
            check(1,golden[1]);
            check(64'h0123456789abcdef,golden[2]);
        end
        $display("PASS 36 HashX generated programs x 3 full 256-bit hashes");
        $finish;
    end
endmodule
