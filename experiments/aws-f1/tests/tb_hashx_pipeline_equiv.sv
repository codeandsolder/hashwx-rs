// Differential fuzz of the timing-oriented multiplier's delay-slot logic.
// Starts from an oracle-checked generated program, mutates valid opcodes,
// and compares the full 256-bit output of the direct and pipelined cores.
// SPDX-License-Identifier: LGPL-3.0-only
module tb_hashx_pipeline_equiv;
    logic clk=0, rst=1, cfg_we=0, start=0;
    logic [10:0] cfg_addr=0;
    logic [31:0] cfg_data=0;
    logic [63:0] nonce=0, target=64'h9999999999999999;
    wire busy_direct, done_direct, hit_direct;
    wire busy_pipe, done_pipe, hit_pipe;
    wire [255:0] digest_direct, digest_pipe;
    logic [31:0] original[0:1032], cfg[0:1032];
    integer pivot=-1, variation=0, total=0;
    integer slot_issues=0;

    hashx_core #(.DIRECT_MUL(1'b1), .PIPE_MUL(1'b0)) direct (
        .clk(clk), .rst(rst), .cfg_we(cfg_we), .cfg_addr(cfg_addr),
        .cfg_data(cfg_data), .start(start), .nonce(nonce), .target(target),
        .busy(busy_direct), .done(done_direct), .hit(hit_direct),
        .digest(digest_direct)
    );
    hashx_core #(.DIRECT_MUL(1'b1), .PIPE_MUL(1'b1)) piped (
        .clk(clk), .rst(rst), .cfg_we(cfg_we), .cfg_addr(cfg_addr),
        .cfg_data(cfg_data), .start(start), .nonce(nonce), .target(target),
        .busy(busy_pipe), .done(done_pipe), .hit(hit_pipe),
        .digest(digest_pipe)
    );
    always #5 clk=~clk;
    always @(posedge clk) begin
        if (!rst && piped.pipe_slot) begin
            slot_issues = slot_issues + 1;
            if ((variation==1 || variation==2) &&
                piped.pc == 10'(pivot+1))
                $fatal(1,"pipeline issued a RAW/WAW-dependent delay slot");
        end
    end

    function automatic logic [63:0] get_inst(input integer index);
        get_inst = {cfg[2*index+1],cfg[2*index]};
    endfunction
    task automatic put_inst(input integer index, input logic [63:0] word);
        cfg[2*index]=word[31:0];
        cfg[2*index+1]=word[63:32];
    endtask
    task automatic load_program;
        for(integer k=0;k<1033;k++) begin
            @(negedge clk);
            cfg_we=1;
            cfg_addr=11'(k);
            cfg_data=cfg[k];
            @(negedge clk); cfg_we=0;
        end
    endtask
    task automatic compare_nonce(input logic [63:0] value);
        integer cycles;
        @(negedge clk);
        nonce=value; start=1;
        @(negedge clk);start=0;
        cycles=0;
        while ((!done_direct || !done_pipe) && cycles<6000) begin
            @(negedge clk);cycles++;
        end
        if(!done_direct || !done_pipe)
            $fatal(1,"pipeline timeout in variation=%0d nonce=%h",variation,value);
        if(digest_direct!==digest_pipe || hit_direct!==hit_pipe)
            $fatal(1,"pipeline mismatch variation=%0d nonce=%h direct=%h pipe=%h",
                variation,value,digest_direct,digest_pipe);
        total++;
    endtask
    initial begin
        $readmemh("tests/fixtures/hashx_config_00.hex",original);
        for(integer i=0;i<int'(original[1032])-1;i++) begin
            logic [63:0] a,b;
            logic [3:0] op_a,op_b;
            a={original[2*i+1],original[2*i]};
            b={original[2*i+3],original[2*i+2]};
            op_a=a[63:60];op_b=b[63:60];
            if(pivot<0 && (op_a<=2 || (op_a>=11 && op_a<=13))
                && op_b>=3 && op_b<=8 && a[59:57]!=b[59:57]
                && !((op_b==3 || op_b==5 || op_b==6)
                     && a[59:57]==b[56:54])) pivot=i;
        end
        if(pivot<0) $fatal(1,"no independent ALU delay slot in fixture");
        repeat(3) @(negedge clk);rst=0;
        for(variation=0;variation<9;variation++) begin
            integer slots_before;
            logic [63:0] first, second;
            slots_before=slot_issues;
            for(integer i=0;i<1033;i++) cfg[i]=original[i];
            first=get_inst(pivot);
            second=get_inst(pivot+1);
            case(variation)
                1: begin
                    // WAW conflict with pending multiply.
                    second[59:57]=first[59:57];
                    put_inst(pivot+1,second);
                end
                2: begin
                    // RAW conflict with pending multiply.
                    second=(64'd6<<60) |
                        (64'((int'(first[59:57])+1)%8)<<57) |
                        (64'(first[59:57])<<54);
                    put_inst(pivot+1,second);
                end
                3: put_inst(pivot+1,64'd9<<60);  // TARGET
                4: put_inst(pivot+1,(64'd10<<60)|64'd1); // BRANCH
                5: put_inst(pivot+1,(64'd0<<60)|(64'd2<<57)|(64'd5<<54));
                6,7: begin
                    put_inst(0,(64'd0<<60)|(64'd0<<57)|(64'd1<<54));
                    put_inst(1,(64'd4<<60)|(64'd2<<57)|64'd99);
                    cfg[1032]=(variation==6) ? 2 : 1;
                end
                8: begin
                    // Last word is an independent delay-slot ALU op.
                    put_inst(int'(cfg[1032])-2,
                        (64'd2<<60)|(64'd0<<57)|(64'd1<<54));
                    put_inst(int'(cfg[1032])-1,
                        (64'd4<<60)|(64'd2<<57)|64'd5);
                end
                default: ;
            endcase
            load_program();
            compare_nonce(0);
            compare_nonce(1);
            compare_nonce(64'hfedcba9876543210);
            if(variation==6 && slot_issues-slots_before!=3)
                $fatal(1,"final delay slot was not filled for 2-word program");
            if(variation==7 && slot_issues!=slots_before)
                $fatal(1,"one-word program illegally issued a delay slot");
        end
        if(slot_issues<2) $fatal(1,"pipeline delay-slot bypass not exercised");
        $display("PASS HashX pipelined register hazard, branch, unfusable, 1/2/352-length and last-slot full-digest differential: %0d runs, %0d filled slots",
            total,slot_issues);
        $finish;
    end
endmodule
