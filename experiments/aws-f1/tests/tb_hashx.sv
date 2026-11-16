module tb_hashx #(
    parameter integer MUL_PARTIALS = 4,
    parameter bit DIRECT_MUL = 1'b1,
    parameter bit PIPE_MUL = 1'b0
);
    logic clk=0, rst=1, cfg_we=0, start=0;
    logic [10:0] cfg_addr=0;
    logic [31:0] cfg_data=0;
    logic [63:0] nonce=0, target=0;
    wire busy, done, hit;
    wire [255:0] digest;
    logic [31:0] cfg [0:1032];
    logic [31:0] unpacked_cfg [0:1032];
    logic [255:0] golden [0:2];
    hashx_core #(.MUL_PARTIALS(MUL_PARTIALS), .DIRECT_MUL(DIRECT_MUL), .PIPE_MUL(PIPE_MUL)) dut(.*);
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
            for(int i=0;i<1033;i++) write_cfg(i,cfg[i]);
            check(0,golden[0]);
            check(1,golden[1]);
            check(64'h0123456789abcdef,golden[2]);
        end
        // Reconstruct the original full 512-instruction stream from
        // the host-packed seed 00. Reconfigure the SAME core from 352
        // packed slots to 512 unpaired slots, checking both the max
        // program-length boundary and branch marker relocation.
        $readmemh("tests/fixtures/hashx_config_00.hex",cfg);
        $readmemh("tests/fixtures/hashx_golden_00.hex",golden);
        begin : unpack_full_program
            integer out_pos;
            logic [63:0] raw, first, second;
            out_pos=0;
            for(integer i=0;i<int'(cfg[1032]);i++) begin
                raw={cfg[i*2+1],cfg[i*2]};
                if(raw[63:60]>=11 && raw[63:60]<=13) begin
                    first = (64'(raw[63:60]-4'd11)<<60)
                            | (raw & (64'h3f << 54));
                    second = (64'(raw[53:50]) << 60)
                            | (64'(raw[49:47]) << 57)
                            | (64'(raw[46:44]) << 54)
                            | (64'(raw[43:38]) << 48)
                            | (64'(raw[37:6]));
                    unpacked_cfg[out_pos*2]=first[31:0];
                    unpacked_cfg[out_pos*2+1]=first[63:32];
                    out_pos++;
                    unpacked_cfg[out_pos*2]=second[31:0];
                    unpacked_cfg[out_pos*2+1]=second[63:32];
                    out_pos++;
                end else begin
                    unpacked_cfg[out_pos*2]=raw[31:0];
                    unpacked_cfg[out_pos*2+1]=raw[63:32];
                    out_pos++;
                end
            end
            if(out_pos!=512) $fatal(1,"incorrect unpacked program size=%0d",out_pos);
        end
        for(integer i=1024;i<1032;i++) unpacked_cfg[i]=cfg[i];
        unpacked_cfg[1032]=512;
        for(integer i=0;i<1033;i++) write_cfg(i,unpacked_cfg[i]);
        check(0,golden[0]);
        check(1,golden[1]);
        check(64'h0123456789abcdef,golden[2]);
        $display("PASS 36 packed HashX programs x 3 full 256-bit hashes, plus 512-instruction uncompressed reload");
        $finish;
    end
endmodule
