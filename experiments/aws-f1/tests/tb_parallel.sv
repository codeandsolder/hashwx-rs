// Parameterized differential tests for wave scheduling, priority, partial waves,
// seed broadcast/reconfiguration, nonce wraparound, and actual-work accounting.
module tb_parallel;
    parameter integer LANES = 4;
    logic clk=0, rst=1;
    logic cfg_we_wx=0, cfg_we_hx=0, start_wx=0, start_hx=0;
    logic [7:0] cfg_addr_wx=0;
    logic [10:0] cfg_addr_hx=0;
    logic [31:0] cfg_data_wx=0, cfg_data_hx=0;
    logic [63:0] nonce_wx=0, nonce_hx=0, target_wx=0, target_hx=0;
    logic [31:0] count_wx=0, count_hx=0;
    wire busy_wx, done_wx, found_wx;
    wire busy_hx, done_hx, found_hx;
    wire [31:0] attempts_wx, attempts_hx;
    wire [63:0] winner_nonce_wx, winner_nonce_hx, winner_hash_wx;
    wire [255:0] winner_digest_hx;
    logic [63:0] wx_golden[0:63], wx_wrap[0:3], wx_seed1[0:2];
    logic [255:0] hx_golden[0:63], hx_wrap[0:3], hx_seed1[0:2];
    logic [31:0] wx_cfg[0:131], hx_cfg[0:1032];
    integer wx_checks=0, hx_checks=0;
    hashwx_scan #(.LANES(LANES)) wx(
        .clk(clk), .rst(rst),
        .cfg_we(cfg_we_wx), .cfg_addr(cfg_addr_wx), .cfg_data(cfg_data_wx),
        .start(start_wx), .first_nonce(nonce_wx), .target(target_wx),
        .nonce_count(count_wx), .busy(busy_wx), .done(done_wx),
        .found(found_wx), .attempts(attempts_wx),
        .winner_nonce(winner_nonce_wx), .winner_hash(winner_hash_wx)
    );
    hashx_scan #(.LANES(LANES)) hx(
        .clk(clk), .rst(rst),
        .cfg_we(cfg_we_hx), .cfg_addr(cfg_addr_hx), .cfg_data(cfg_data_hx),
        .start(start_hx), .first_nonce(nonce_hx), .target(target_hx),
        .nonce_count(count_hx), .busy(busy_hx), .done(done_hx),
        .found(found_hx), .attempts(attempts_hx),
        .winner_nonce(winner_nonce_hx), .winner_digest(winner_digest_hx)
    );
    always #5 clk=~clk;

    task automatic load_wx(input integer index);
        $readmemh($sformatf("tests/fixtures/config_%02d.hex",index),wx_cfg);
        for(integer k=0;k<132;k++) begin
            @(negedge clk);
            cfg_we_wx=1;
            cfg_addr_wx=k[7:0];
            cfg_data_wx=wx_cfg[k];
            @(negedge clk);
            cfg_we_wx=0;
        end
    endtask
    task automatic load_hx(input integer index);
        $readmemh($sformatf("tests/fixtures/hashx_config_%02d.hex",index),hx_cfg);
        for(integer k=0;k<1033;k++) begin
            @(negedge clk);
            cfg_we_hx=1;
            cfg_addr_hx=k[10:0];
            cfg_data_hx=hx_cfg[k];
            @(negedge clk);
            cfg_we_hx=0;
        end
    endtask

    task automatic expect_wx(input integer base, input integer count,
                             input logic [63:0] threshold);
        integer first_hit, expected_attempts, cycles;
        first_hit=-1;
        for (integer i=0;i<count;i++)
            if (first_hit<0 && wx_golden[base+i]<threshold) first_hit=i;
        expected_attempts = first_hit<0 ? count
            : ((first_hit/LANES)+1)*LANES;
        if (expected_attempts>count) expected_attempts=count;
        @(negedge clk);
        nonce_wx=64'(base); count_wx=32'(count); target_wx=threshold;
        start_wx=1;
        @(negedge clk);
        start_wx=0;
        cycles=0;
        while(!done_wx && cycles<1000000) begin @(negedge clk); cycles++; end
        if(!done_wx) $fatal(1,"WX timeout lanes=%0d base=%0d count=%0d",LANES,base,count);
        if(attempts_wx!==expected_attempts ||
           found_wx!==(first_hit>=0))
            $fatal(1,"WX lanes=%0d base=%0d count=%0d expected attempts=%0d found=%b got %0d %b",
                LANES,base,count,expected_attempts,(first_hit>=0),
                attempts_wx,found_wx);
        if(first_hit>=0 && (winner_nonce_wx!==64'(base+first_hit) ||
           winner_hash_wx!==wx_golden[base+first_hit]))
            $fatal(1,"WX first-hit ordering or digest incorrect lanes=%0d base=%0d",
                LANES,base);
        wx_checks++;
        if (count == 16 && threshold == 0)
            $display("BENCH WX LANES=%0d nonces=%0d cycles=%0d", LANES, count, cycles);
    endtask
    task automatic expect_hx(input integer base, input integer count,
                             input logic [63:0] threshold);
        integer first_hit, expected_attempts, cycles;
        first_hit=-1;
        for (integer i=0;i<count;i++)
            if (first_hit<0 && hx_golden[base+i][63:0]<threshold) first_hit=i;
        expected_attempts = first_hit<0 ? count
            : ((first_hit/LANES)+1)*LANES;
        if(expected_attempts>count) expected_attempts=count;
        @(negedge clk);
        nonce_hx=64'(base); count_hx=32'(count); target_hx=threshold;
        start_hx=1;
        @(negedge clk);
        start_hx=0;
        cycles=0;
        while(!done_hx && cycles<400000) begin @(negedge clk); cycles++; end
        if(!done_hx) $fatal(1,"HX timeout lanes=%0d base=%0d count=%0d",LANES,base,count);
        if(attempts_hx!==expected_attempts ||
           found_hx!==(first_hit>=0))
            $fatal(1,"HX lanes=%0d base=%0d count=%0d expected attempts=%0d found=%b got %0d %b",
                LANES,base,count,expected_attempts,(first_hit>=0),
                attempts_hx,found_hx);
        if(first_hit>=0 && (winner_nonce_hx!==64'(base+first_hit) ||
           winner_digest_hx!==hx_golden[base+first_hit]))
            $fatal(1,"HX first-hit ordering or digest incorrect lanes=%0d base=%0d",
                LANES,base);
        hx_checks++;
        if (count == 16 && threshold == 0)
            $display("BENCH HX LANES=%0d nonces=%0d cycles=%0d", LANES, count, cycles);
    endtask

    task automatic expect_later_wx;
        integer base, k;
        logic [63:0] prefix_min, suffix_min, threshold;
        base=-1;
        threshold=0;
        for(integer i=0;i+2*LANES<=64;i++) begin
            prefix_min=64'hffffffffffffffff;
            suffix_min=64'hffffffffffffffff;
            for(k=0;k<LANES;k++) begin
                if(wx_golden[i+k]<prefix_min) prefix_min=wx_golden[i+k];
                if(wx_golden[i+LANES+k]<suffix_min)
                    suffix_min=wx_golden[i+LANES+k];
            end
            if(base<0 && suffix_min<prefix_min) begin
                base=i;threshold=prefix_min;
            end
        end
        if(base<0) $fatal(1,"WX fixtures lack second-wave win, LANES=%0d",LANES);
        expect_wx(base,2*LANES,threshold);
    endtask
    task automatic expect_later_hx;
        integer base,k;
        logic [63:0] prefix_min, suffix_min, threshold;
        base=-1;threshold=0;
        for(integer i=0;i+2*LANES<=64;i++) begin
            prefix_min=64'hffffffffffffffff;
            suffix_min=64'hffffffffffffffff;
            for(k=0;k<LANES;k++) begin
                if(hx_golden[i+k][63:0]<prefix_min)
                    prefix_min=hx_golden[i+k][63:0];
                if(hx_golden[i+LANES+k][63:0]<suffix_min)
                    suffix_min=hx_golden[i+LANES+k][63:0];
            end
            if(base<0 && suffix_min<prefix_min) begin
                base=i;threshold=prefix_min;
            end
        end
        if(base<0) $fatal(1,"HX fixtures lack second-wave win, LANES=%0d",LANES);
        expect_hx(base,2*LANES,threshold);
    endtask

    task automatic expect_wrap;
        integer cycles;
        @(negedge clk);
        nonce_wx=64'hfffffffffffffffe;
        count_wx=3;
        target_wx=64'hffffffffffffffff;
        start_wx=1;
        @(negedge clk);
        start_wx=0;
        cycles=0;
        while(!done_wx && cycles<100000) begin @(negedge clk); cycles++; end
        if(!done_wx || !found_wx || winner_nonce_wx!==64'hfffffffffffffffe ||
           winner_hash_wx!==wx_wrap[0] || attempts_wx!==(LANES>=3?3:LANES))
            $fatal(1,"WX wrapped nonce mismatch lanes=%0d",LANES);
        @(negedge clk);
        nonce_hx=64'hfffffffffffffffe;
        count_hx=3;
        target_hx=64'hffffffffffffffff;
        start_hx=1;
        @(negedge clk);
        start_hx=0;
        cycles=0;
        while(!done_hx && cycles<100000) begin @(negedge clk); cycles++; end
        if(!done_hx || !found_hx || winner_nonce_hx!==64'hfffffffffffffffe ||
           winner_digest_hx!==hx_wrap[0] || attempts_hx!==(LANES>=3?3:LANES))
            $fatal(1,"HX wrapped nonce mismatch lanes=%0d",LANES);
        wx_checks++;hx_checks++;
    endtask

    initial begin
        $readmemh("tests/fixtures/hashwx_scan_golden.hex",wx_golden);
        $readmemh("tests/fixtures/hashx_scan_golden.hex",hx_golden);
        $readmemh("tests/fixtures/hashwx_wrap_golden.hex",wx_wrap);
        $readmemh("tests/fixtures/hashx_wrap_golden.hex",hx_wrap);
        $readmemh("tests/fixtures/golden_01.hex",wx_seed1);
        $readmemh("tests/fixtures/hashx_golden_01.hex",hx_seed1);
        repeat(3) @(negedge clk);
        rst=0;
        load_wx(0);load_hx(0);
        expect_wx(0,0,64'hffffffffffffffff);
        expect_hx(0,0,64'hffffffffffffffff);
        expect_wx(0,2*LANES+1,0);
        expect_hx(0,2*LANES+1,0);
        expect_wx(0,LANES+1,0);
        expect_hx(0,LANES+1,0);
        expect_wx(0,16,0);
        expect_hx(0,16,0);
        expect_wx(0,LANES+2,64'hffffffffffffffff);
        expect_hx(0,LANES+2,64'hffffffffffffffff);
        expect_later_wx();
        expect_later_hx();
        expect_wrap();
        // All lanes must receive a new program (not just the first lane).
        load_wx(1);load_hx(1);
        @(negedge clk);
        nonce_wx=0;count_wx=1;target_wx=wx_seed1[0]+1'b1;start_wx=1;
        @(negedge clk);start_wx=0;
        wait(done_wx);
        if(!found_wx || winner_hash_wx!==wx_seed1[0]) $fatal(1,"WX program reload");
        @(negedge clk);
        nonce_hx=0;count_hx=1;target_hx=hx_seed1[0][63:0]+1'b1;start_hx=1;
        @(negedge clk);start_hx=0;
        wait(done_hx);
        if(!found_hx || winner_digest_hx!==hx_seed1[0]) $fatal(1,"HX program reload");
        $display("PASS parallel LANES=%0d: WX=%0d HX=%0d oracle-verified scans, multiwave, partial, priority, wrap and reconfiguration",
            LANES,wx_checks+1,hx_checks+1);
        $finish;
    end
endmodule
