module tb_hashwx;
    logic clk=0, rst=1, cfg_we=0, start=0;
    logic [7:0] cfg_addr=0;
    logic [31:0] cfg_data=0;
    logic [63:0] nonce=0, target=0;
    wire busy, done, hit;
    wire [63:0] hash;
    logic start_scan=0;
    logic [63:0] first_nonce=0, scan_target=0;
    logic [31:0] nonce_count=0;
    wire scan_busy, scan_done, found;
    wire [63:0] winner_nonce, winner_hash;
    wire [31:0] attempts;
    logic [31:0] cfg [0:131];
    logic [63:0] golden [0:2];
    hashwx_core dut(.*);
    hashwx_scan scanner(
        .clk(clk),.rst(rst),.cfg_we(cfg_we),
        .cfg_addr(cfg_addr),.cfg_data(cfg_data),
        .start(start_scan),.first_nonce(first_nonce),.target(scan_target),
        .nonce_count(nonce_count),.busy(scan_busy),.done(scan_done),.found(found),
        .attempts(attempts),.winner_nonce(winner_nonce),.winner_hash(winner_hash)
    );
    always #5 clk=~clk;

    task automatic write_cfg(input integer addr, input logic [31:0] value);
        @(negedge clk);
        cfg_we = 1;
        cfg_addr = addr[7:0];
        cfg_data = value;
        @(negedge clk);
        cfg_we = 0;
    endtask

    task automatic check_hash(input logic [63:0] candidate, input logic [63:0] expected);
        integer cycles;
        @(negedge clk);
        nonce = candidate;
        target = expected + 1'b1;
        start = 1;
        @(negedge clk);
        start = 0;
        cycles = 0;
        while (!done && cycles < 2000000) begin
            @(negedge clk);
            cycles++;
        end
        if (!done) $fatal(1,"timeout nonce=%h",candidate);
        if (hash !== expected) $fatal(1,"nonce=%h expected=%h actual=%h",candidate,expected,hash);
        if (!hit) $fatal(1,"expected hash below exclusive target");
        if (dut.snapshot_count != 256) $fatal(1,"unexpected snapshot_count=%d",dut.snapshot_count);
        $display("PASS nonce=%016h hash=%016h cycles=%0d",candidate,hash,cycles);
    endtask

    task automatic check_scan(input logic [31:0] count, input logic [63:0] threshold,
                              input logic expected_found, input logic [63:0] expected_nonce,
                              input logic [31:0] expected_attempts);
        integer cycles;
        @(negedge clk);
        first_nonce = 0;
        nonce_count = count;
        scan_target = threshold;
        start_scan = 1;
        @(negedge clk);
        start_scan = 0;
        cycles = 0;
        while (!scan_done && cycles < 1000000) begin
            @(negedge clk);
            cycles++;
        end
        if (!scan_done) $fatal(1,"scan timed out");
        if (found !== expected_found || attempts !== expected_attempts)
            $fatal(1,"scan expected found=%d attempts=%d got found=%d attempts=%d",
                   expected_found,expected_attempts,found,attempts);
        if (expected_found && (winner_nonce !== expected_nonce ||
             winner_hash !== golden[0])) $fatal(1,"scan wrong winner %h: %h",winner_nonce,winner_hash);
        $display("PASS scan count=%0d attempts=%0d found=%b",count,attempts,found);
    endtask

    initial begin
        $readmemh("tests/fixtures/config.hex",cfg);
        $readmemh("tests/fixtures/golden.hex",golden);
        repeat(3) @(negedge clk);
        rst = 0;
        for (int i=0;i<132;i++) write_cfg(i,cfg[i]);
        check_hash(0,golden[0]);
        check_hash(1,golden[1]);
        check_hash(64'h0123456789abcdef,golden[2]);
        // Exact target=0 never matches. A count=0 request never hashes.
        check_scan(3,0,0,0,3);
        check_scan(0,0,0,0,0);
        // Exclusive comparison and early success (nonce zero has hash golden[0]).
        check_scan(1,golden[0],0,0,1);
        check_scan(3,golden[0]+64'd1,1,0,3);
        // Different unrelated program seeds, including many byte values > 127.
        for (int seed_idx=1; seed_idx<16; seed_idx++) begin
            $readmemh($sformatf("tests/fixtures/config_%02d.hex",seed_idx),cfg);
            $readmemh($sformatf("tests/fixtures/golden_%02d.hex",seed_idx),golden);
            for (int c=0;c<132;c++) write_cfg(c,cfg[c]);
            check_hash(0,golden[0]);
            check_hash(1,golden[1]);
            check_hash(64'h0123456789abcdef,golden[2]);
        end
        $display("PASS: 16 program sets * 3 independent hashes, scan boundaries and success");
        $finish;
    end
endmodule
