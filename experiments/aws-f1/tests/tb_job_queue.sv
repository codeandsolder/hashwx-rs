// Non-power-of-two depth, full backpressure, FIFO ordering and result
// retention under a deterministic mock scanner. This validates the FIFO
// independently of slower cryptographic cores.
module tb_job_queue;
    logic clk=0,rst=1,command_valid=0,result_pop=0;
    logic [63:0] command_nonce=0,command_target=0;
    logic [31:0] command_count=0,command_tag=0;
    wire command_ready,config_idle,scanner_start;
    wire [63:0] scanner_nonce,scanner_target;
    wire [31:0] scanner_count;
    logic scanner_busy=0,scanner_done=0,scanner_found=0;
    logic [31:0] scanner_attempts=0;
    logic [63:0] scanner_winner_nonce=0;
    logic [255:0] scanner_winner_hash=0;
    wire result_valid,result_found;
    wire [31:0] result_tag,result_attempts;
    wire [63:0] result_nonce;
    wire [255:0] result_hash;
    wire [15:0] queued_commands,queued_results;
    logic [2:0] mock_ticks=0;
    logic [31:0] completed=0;

    hash_job_queue #(.HASH_BITS(256),.DEPTH(3)) dut (.*);
    always #5 clk=~clk;

    always_ff @(posedge clk) begin
        if(rst) begin
            scanner_busy<=0;scanner_done<=0;mock_ticks<=0;
            scanner_found<=0;scanner_attempts<=0;
            scanner_winner_nonce<=0;scanner_winner_hash<=0;
            completed<=0;
        end else if(scanner_start) begin
            scanner_busy<=1;
            scanner_done<=0;
            mock_ticks<=0;
        end else if(scanner_busy) begin
            if(mock_ticks==3) begin
                scanner_busy<=0;
                scanner_done<=1;
                scanner_found<=1;
                scanner_attempts<=scanner_count;
                scanner_winner_nonce<=scanner_nonce;
                scanner_winner_hash<={192'd0,scanner_nonce^scanner_target};
                completed<=completed+1;
            end else mock_ticks<=mock_ticks+1;
        end
    end

    task automatic enqueue(input integer tag);
        integer cycles;
        cycles=0;
        @(negedge clk);
        while(!command_ready && cycles<1000) begin
            @(negedge clk);cycles++;
        end
        if(!command_ready) $fatal(1,"unexpected input FIFO full tag=%0d",tag);
        command_valid=1;
        command_nonce=64'(tag*10);
        command_target=64'(tag*71);
        command_count=32'(tag+1);
        command_tag=32'(tag);
        @(negedge clk);
        command_valid=0;
    endtask

    task automatic check_result(input integer tag);
        integer cycles;
        cycles=0;
        @(negedge clk);
        while(!result_valid && cycles<1000) begin
            @(negedge clk);cycles++;
        end
        if(!result_valid) $fatal(1,"missing completion %0d",tag);
        if(result_tag!==32'(tag) || result_attempts!==32'(tag+1) ||
            result_nonce!==64'(tag*10) || !result_found ||
            result_hash!=={192'd0,(64'(tag*10)^64'(tag*71))})
            $fatal(1,"wrong completion order/result tag=%0d got %0d attempts=%0d",
                   tag,result_tag,result_attempts);
        result_pop=1;
        @(negedge clk);
        result_pop=0;
    endtask

    initial begin
        repeat(3) @(negedge clk);
        rst=0;
        if(!config_idle || !command_ready || result_valid)
            $fatal(1,"invalid reset state");
        enqueue(1);enqueue(2);enqueue(3);enqueue(4);
        // At least one job has started and the request queue should
        // now be full. No extra command may be accepted.
        if(command_ready || queued_commands!==3)
            $fatal(1,"missing command FIFO backpressure pending=%0d",queued_commands);
        @(negedge clk);
        command_valid=1;command_tag=999;
        @(negedge clk);
        command_valid=0; // intentionally rejected
        repeat(100) @(negedge clk);
        if(queued_results!==3 || completed!==3 ||
           !result_valid || result_tag!==1 || queued_commands!==1)
            $fatal(1,"result FIFO must block fourth dispatch, got res=%0d comp=%0d cmd=%0d",
                queued_results,completed,queued_commands);
        check_result(1);
        check_result(2);
        check_result(3);
        check_result(4);
        repeat(3) @(negedge clk);
        if(!config_idle || result_valid || queued_commands!==0 || queued_results!==0)
            $fatal(1,"queue failed to drain cleanly");
        enqueue(5);
        check_result(5);
        $display("PASS queue depth=3: FIFO order, full-command reject, result backpressure, tags, re-enqueue");
        $finish;
    end
endmodule
