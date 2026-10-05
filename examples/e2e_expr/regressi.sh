#!/bin/sh
# Sweep regresi: cek berbagai bentuk SystemVerilog, laporkan yang masih gagal.
cd /home/whale-d/svrust
SV="cargo run -q -p cargo-sv -- sv"

probe() {
  printf '%s\n' "$2" > /tmp/p.sv
  out=$($SV check --rtl /tmp/p.sv 2>&1 | grep -E "^(Error|check OK)" | head -1)
  case "$out" in
    check\ OK*) printf '  OK     %s\n' "$1" ;;
    *)          printf '  GAGAL  %-28s %s\n' "$1" "$out" ;;
  esac
}

echo "--- sized literal ---"
probe "8'hFF"            'module m(output [7:0] y); assign y = 8'"'"'hFF; endmodule'
probe "4'b1010_1010"     'module m(output [7:0] y); assign y = 8'"'"'b1010_1010; endmodule'
probe "8'd200"           'module m(output [7:0] y); assign y = 8'"'"'d200; endmodule'
probe "'hFF unsized"     'module m(output [7:0] y); assign y = '"'"'hFF; endmodule'
probe "32'hDEAD_BEEF"    'module m(output [31:0] y); assign y = 32'"'"'hDEAD_BEEF; endmodule'
probe "1'b0 / 1'b1"      'module m(input a, output y); assign y = a ? 1'"'"'b1 : 1'"'"'b0; endmodule'
probe "literal di case"  'module m(input [3:0] a, output [3:0] y); always_comb case (a) 4'"'"'d1: y = 1; default: y = 0; endcase endmodule'
probe "literal di param" 'module m #(parameter W = 8'"'"'d4) (output [W-1:0] y); assign y = 0; endmodule'
probe "desimal polos"    'module m(output [7:0] y); assign y = 42; endmodule'

echo "--- unary & reduction ---"
probe "~a"               'module m(input [3:0] a, output [3:0] y); assign y = ~a; endmodule'
probe "-a"               'module m(input [3:0] a, output [3:0] y); assign y = -a; endmodule'
probe "!a"               'module m(input a, output y); assign y = !a; endmodule'
probe "&a |a ^a"         'module m(input [3:0] a, output [2:0] y); assign y[0] = &a; assign y[1] = |a; assign y[2] = ^a; endmodule'
probe "~&a ~|a ~^a"      'module m(input [3:0] a, output [2:0] y); assign y[0] = ~&a; assign y[1] = ~|a; assign y[2] = ~^a; endmodule'
probe "nested unary"     'module m(input [3:0] a, output [3:0] y); assign y = ~~a; endmodule'
probe "unary di if"      'module m(input [3:0] a, output y); always_comb if (&a) y = 1'"'"'b1; else y = 1'"'"'b0; endmodule'

echo "--- ternary ---"
probe "ternary"          'module m(input s, output [7:0] y); assign y = s ? 8'"'"'hFF : 8'"'"'h00; endmodule'
probe "ternary nested"   'module m(input a, input b, output [7:0] y); assign y = a ? (b ? 1 : 2) : 3; endmodule'
probe "ternary di case"  'module m(input [1:0] s, output [7:0] y); always_comb case (s) 0: y = s ? 1 : 2; default: y = 0; endcase endmodule'
probe "ternary sinyal"   'module m(input s, input [7:0] a, input [7:0] b, output [7:0] y); assign y = s ? a : b; endmodule'

echo "--- select ---"
probe "bit select"       'module m(input [7:0] a, output y); assign y = a[0]; endmodule'
probe "part select"      'module m(input [7:0] a, output [3:0] y); assign y = a[7:4]; endmodule'
probe "select di expr"   'module m(input [7:0] a, input [7:0] b, output [3:0] y); assign y = a[3:0] + b[2:0]; endmodule'
probe "select di lhs"    'module m(input [7:0] a, output [7:0] y); assign y[3:0] = a[7:4]; endmodule'
probe "select di unary"  'module m(input [7:0] a, output [3:0] y); assign y = ~a[7:4]; endmodule'

echo "--- qualifier port ---"
probe "input logic"      'module m(input logic [3:0] a, output [3:0] y); assign y = a; endmodule'
probe "output reg"       'module m(input clk, input d, output reg q); always_ff @(posedge clk) q <= d; endmodule'
probe "output reg leb"   'module m(input [3:0] a, output reg [3:0] y); always_comb y = a; endmodule'
probe "inout wire"       'module m(inout wire [3:0] p, output [3:0] y); assign y = p; endmodule'

echo "--- signed ---"
probe "signed port"      'module m(input signed [3:0] a, output signed [3:0] y); assign y = a; endmodule'
probe "signed decl"      'module m(output [3:0] y); logic signed [7:0] t; assign y = t; endmodule'
probe "unsigned ekspl"   'module m(input unsigned [7:0] a, output [7:0] y); assign y = a; endmodule'

echo "--- concat & replikasi ---"
probe "concat dua"       'module m(input [3:0] a, input [3:0] b, output [7:0] y); assign y = {a,b}; endmodule'
probe "concat tiga"      'module m(input [1:0] a, input [1:0] b, input [1:0] c, output [5:0] y); assign y = {a,b,c}; endmodule'
probe "concat literal"   'module m(output [7:0] y); assign y = {4'"'"'hA, 4'"'"'h3}; endmodule'
probe "replikasi"       'module m(output [7:0] y); assign y = {4{2'"'"'b10}}; endmodule'
probe "repl nested"      'module m(output [7:0] y); assign y = {4'"'"'h0, {2{2'"'"'b11}}}; endmodule'
probe "concat + select"  'module m(input [7:0] a, output [3:0] y); assign y = {2'"'"'h0, a[1:0]}; endmodule'

echo "--- casez / casex ---"
probe "casez wildcard"   'module m(input [1:0] s, output [3:0] y); always_comb casez(s) 2'"'"'b1?: y = 4'"'"'d1; default: y = 4'"'"'d0; endcase endmodule'
probe "casez z wildcard" 'module m(input [1:0] s, output [3:0] y); always_comb casez(s) 2'"'"'b1z: y = 4'"'"'d1; default: y = 4'"'"'d0; endcase endmodule'
probe "casex x wildcard" 'module m(input [1:0] s, output [3:0] y); always_comb casex(s) 2'"'"'b1x: y = 4'"'"'d1; default: y = 4'"'"'d0; endcase endmodule'
probe "casez semua ?"    'module m(input [1:0] s, output [3:0] y); always_comb casez(s) 2'"'"'b??: y = 4'"'"'d1; default: y = 4'"'"'d0; endcase endmodule'
probe "case tanpa default" 'module m(input [1:0] s, output [3:0] y); always_comb casez(s) 2'"'"'b1?: y = 4'"'"'d1; 2'"'"'b01: y = 4'"'"'d2; endcase endmodule'
probe "literal x di luar casez" 'module m(output [3:0] y); assign y = 4'"'"'bx1; endmodule'

echo "--- pembagian & modulo ---"
probe "a / b"            'module m(input [7:0] a, input [7:0] b, output [7:0] y); assign y = a / b; endmodule'
probe "a % b"            'module m(input [7:0] a, input [7:0] b, output [7:0] y); assign y = a % b; endmodule'
probe "bagi pembagi nol"  'module m(input [7:0] a, output [7:0] y); assign y = a / 8'"'"'d0; endmodule'
probe "sisa pembagi nol"  'module m(input [7:0] a, output [7:0] y); assign y = a % 8'"'"'d0; endmodule'
probe "bagi presedensi"   'module m(output [7:0] y); assign y = 100 / 5 * 2; endmodule'
probe "bagi kurung"       'module m(input [7:0] a, output [7:0] y); assign y = 100 / (a + 1); endmodule'
probe "a /= b"            'module m(input [7:0] a, input [7:0] b, output [7:0] y); always_comb begin a = 8'"'"'d100; a /= b; y = a; end endmodule'
probe "a %= b"            'module m(input [7:0] a, input [7:0] b, output [7:0] y); always_comb begin a = 8'"'"'d100; a %= b; y = a; end endmodule'
probe "sisa di if"        'module m(input [7:0] a, output y); always_comb if (a % 2 == 0) y = 1'"'"'b1; else y = 1'"'"'b0; endmodule'
probe "bagi di case"      'module m(input [7:0] a, output [3:0] y); always_comb case (a / 8) 0: y = 1; default: y = 0; endcase endmodule'

echo "--- deklarator majemuk ---"
probe "logic a, b"        'module m(output [7:0] y); logic [7:0] a, b; always_comb begin a=1; b=2; y=a+b; end endmodule'
probe "logic a, b, c"     'module m(output [7:0] y); logic [7:0] a, b, c; always_comb begin a=1; b=2; c=4; y=a|b|c; end endmodule'
probe "wire a, b"         'module m(output [7:0] y); wire [7:0] a, b; assign y = a | b; endmodule'
probe "signed a, b"       'module m(output [7:0] y); logic signed [7:0] a, b; assign y = a + b; endmodule'
probe "majemuk + bagi"    'module m(output [7:0] y); logic [7:0] a, b, c; always_comb begin a=100; b=7; c=3; y = a / b % c; end endmodule'

echo "--- deklarasi lokal & langkah loop ---"
probe "lokal logic"      'module m(output [7:0] y); always_comb begin logic [7:0] t; t=8'"'"'d5; y=t; end endmodule'
probe "lokal majemuk"    'module m(output [7:0] y); always_comb begin logic [3:0] a, b; a=4'"'"'d2; b=4'"'"'d3; y={a,b}; end endmodule'
probe "lokal di if"      'module m(output [7:0] y); always_comb begin logic [7:0] t; if (1'"'"'b1) t=8'"'"'d1; else t=8'"'"'d2; y=t; end endmodule'
probe "lokal di for"     'module m(output [7:0] y); logic [3:0] k; always_comb begin logic [7:0] s; s=0; for (k=0;k<4;k++) s=s+k; y=s; end endmodule'
probe "lokal di always_ff" 'module m(input clk, input d, output reg q); always_ff @(posedge clk) begin logic t; t <= d; q <= t; end endmodule'
probe "lokal sama 2 blok" 'module m(output [7:0] y, output [7:0] z); always_comb begin logic [7:0] t; t=1; y=t; end always_comb begin logic [7:0] t; t=2; z=t; end endmodule'
probe "lokal di instans"  'module c(input [7:0] a, output [7:0] y); always_comb begin logic [7:0] t; t=a; y=t; end endmodule module t2(input [7:0] a, output [7:0] z); c u(.a(a), .y(z)); endmodule'
probe "for step i++"      'module m(output [7:0] y); logic [3:0] i; logic [7:0] s; always_comb begin s=0; for (i=0;i<5;i++) s=s+1; y=s; end endmodule'
probe "for step i--"      'module m(output [7:0] y); logic [3:0] i; logic [7:0] s; always_comb begin s=0; for (i=9;i>0;i--) s=s+1; y=s; end endmodule'
probe "for step i+=2"     'module m(output [7:0] y); logic [3:0] i; logic [7:0] s; always_comb begin s=0; for (i=0;i<8;i+=2) s=s+1; y=s; end endmodule'
probe "for step i/=2"     'module m(output [7:0] y); logic [3:0] i; logic [7:0] s; always_comb begin s=0; for (i=8;i>0;i/=2) s=s+1; y=s; end endmodule'
probe "for body kosong"   'module m(output [7:0] y); logic [3:0] i; always_comb begin for (i=0;i<4;i=i+1); y=i; end endmodule'
probe "blocking di ff"    'module m(input clk, input d, output reg q); always_ff @(posedge clk) begin q = d; end endmodule'
probe "blocking+nonblok"  'module m(input clk, input d, output reg q); always_ff @(posedge clk) begin logic t; t = d; q <= t; end endmodule'

echo "--- while, integer, inisialisasi ---"
probe "while dasar"       'module m(output [7:0] y); logic [3:0] i; always_comb begin i=0; while (i<4) i=i+1; y=i; end endmodule'
probe "while body blok"   'module m(output [7:0] y); logic [3:0] i; always_comb begin y=0; i=0; while (i<3) begin y=y+1; i=i+1; end end endmodule'
probe "while kosong"      'module m(output [7:0] y); logic [3:0] i; always_comb begin while (i<4); y=1; end endmodule'
probe "while di instans"  'module c(input [3:0] n, output [3:0] y); always_comb begin logic [3:0] i; i=0; while (i<n) i=i+1; y=i; end endmodule module t3(input [3:0] n, output [3:0] z); c u(.n(n), .y(z)); endmodule'
probe "integer modul"     'module m(output [31:0] y); integer s; assign y = s; endmodule'
probe "int modul"         'module m(output [31:0] y); int s; assign y = s; endmodule'
probe "integer lokal"     'module m(output [7:0] y); always_comb begin integer s; s=5; y=s[7:0]; end endmodule'
probe "integer majemuk"   'module m(output [31:0] y); integer a, b; assign y = a + b; endmodule'
probe "nilai awal var"    'module m(output [7:0] y); logic [7:0] s = 8'"'"'d42; assign y = s; endmodule'
probe "nilai awal lokal"  'module m(output [7:0] y); always_comb begin logic [7:0] s = 8'"'"'d9; y = s; end endmodule'
probe "nilai awal majemuk" 'module m(output [7:0] y); logic [7:0] a = 8'"'"'d1, b = 8'"'"'d2; assign y = a | b; endmodule'
probe "net decl assign"   'module m(input [7:0] a, output [7:0] y); wire [7:0] w = a; assign y = w; endmodule'

echo "--- increment statement ---"
probe "i++ statement"     'module m(output [7:0] y); logic [3:0] i; always_comb begin i=0; i++; y=i; end endmodule'
probe "i-- statement"     'module m(output [7:0] y); logic [3:0] i; always_comb begin i=5; i--; y=i; end endmodule'
probe "i++ di while"      'module m(output [7:0] y); logic [3:0] i; always_comb begin y=0; i=0; while (i<4) i++; y=i; end endmodule'
probe "i++ di always_ff"  'module m(input clk, output reg [3:0] c); always_ff @(posedge clk) begin c++; end endmodule'
probe "i++ bertumpuk"     'module m(output [7:0] y); logic [3:0] i; always_comb begin i=0; i++; i++; y=i; end endmodule'

echo "--- repeat & pre-increment ---"
probe "repeat sinyal"     'module m(output [7:0] y); integer n; always_comb begin y=0; n=3; repeat (n) y=y+1; end endmodule'
probe "repeat literal"    'module m(output [7:0] y); always_comb begin y=0; repeat (4) y=y+2; end endmodule'
probe "repeat body blok"  'module m(output [7:0] y); integer n; always_comb begin y=0; n=2; repeat (n) begin y=y+1; y=y+10; end end endmodule'
probe "repeat kosong"     'module m(output [7:0] y); always_comb begin repeat (2); y=1; end endmodule'
probe "repeat di instans" 'module c(input integer n, output [7:0] y); always_comb begin y=0; repeat (n) y=y+1; end endmodule module t4(input integer n, output [7:0] z); c u(.n(n), .y(z)); endmodule'
probe "++a; prefix"       'module m(output [7:0] y); logic [3:0] a; always_comb begin a=1; ++a; y=a; end endmodule'
probe "--a; prefix"       'module m(output [7:0] y); logic [3:0] a; always_comb begin a=5; --a; y=a; end endmodule'
probe "++a; di always_ff" 'module m(input clk, output reg [3:0] c); always_ff @(posedge clk) begin ++c; end endmodule'
probe "port integer"      'module m(input integer n, output [31:0] y); assign y = n; endmodule'

echo "--- testbench: initial, system task ---"
probe "initial blok"      'module tb; logic [7:0] a; initial begin a = 8'"'"'d42; end endmodule'
probe "initial satu stmt" 'module tb; logic [7:0] a; initial a = 8'"'"'d42; endmodule'
probe "display teks"      'module tb; initial begin $display("halo"); end endmodule'
probe "display %d"        'module tb; logic [7:0] a; initial begin a = 8'"'"'d1; $display("a=%d", a); end endmodule'
probe "display %h"        'module tb; logic [3:0] a; initial begin a = 4'"'"'hA; $display("a=%h", a); end endmodule'
probe "display %b"        'module tb; logic [3:0] a; initial begin a = 4'"'"'hA; $display("a=%b", a); end endmodule'
probe "display multi arg" 'module tb; logic [7:0] a; logic [7:0] b; initial begin a=1; b=2; $display("a=%d b=%d", a, b); end endmodule'
probe "display tanpa arg" 'module tb; initial begin $display("halo"); $finish; end endmodule'
probe "display newline"   'module tb; initial begin $display("a\nb"); end endmodule'
probe "display %0Nd"      'module tb; logic [7:0] a; initial begin a=7; $display("a=%03d", a); end endmodule'
probe "display literal"   'module tb; initial begin $display(42); end endmodule'
probe "finish tanpa kurung" 'module tb; initial begin $finish; end endmodule'
probe "finish dengan kurung" 'module tb; initial begin $finish(); end endmodule'
probe "display signed"    'module tb; logic signed [7:0] s; initial begin s = -8'"'"'d1; $display("s=%d", s); end endmodule'
probe "display expr"      'module tb; logic [7:0] a; logic [7:0] b; initial begin a=40; b=2; $display("c=%d", a+b); end endmodule'
probe "display di loop"   'module tb; logic [3:0] i; initial begin for (i=0;i<3;i=i+1) $display("i=%d", i); end endmodule'
probe "display lokal"     'module tb; initial begin logic [7:0] t; t=8'"'"'d5; $display("t=%d", t); end endmodule'
probe "display di instans" 'module c(input [7:0] a); initial $display("a=%d", a); endmodule module t2(input [7:0] z); c u(.a(z)); endmodule'

# Pesan error yang diharapkan: system task di luar daftar yang didukung.
probe_gagal() {
  printf '%s\n' "$2" > /tmp/p.sv
  out=$($SV check --rtl /tmp/p.sv 2>&1 | grep -E "^Error" | head -1)
  case "$out" in
    *"$3"*) printf '  OK     %s\n' "$1" ;;
    *)       printf '  GAGAL  %-28s %s\n' "$1" "$out" ;;
  esac
}

probe_gagal "task tak dikenal pesan jelas" 'module tb; logic [7:0] a; initial begin $writeln(a); end endmodule' "belum didukung"

echo "--- penundaan waktu: #delay dan \$time ---"
probe "delay konstanta"    'module tb; logic [7:0] a; initial begin #5 a = 8'"'"'d1; end endmodule'
probe "delay tanpa body"   'module tb; logic [7:0] a; initial begin #10; a = 8'"'"'d1; end endmodule'
probe "delay satuan ns"    'module tb; logic [7:0] a; initial begin #3ns a = 8'"'"'d1; end endmodule'
probe "delay satuan ps"    'module tb; logic [7:0] a; initial begin #1ps a = 8'"'"'d1; end endmodule'
probe "delay sinyal"       'module tb; integer n; logic [7:0] a; initial begin n = 4; #n a = 8'"'"'d1; end endmodule'
probe "delay blok"         'module tb; logic [7:0] a; initial begin #1 begin a = 1; a = 2; end end endmodule'
probe "delay dalam loop"   'module tb; logic [3:0] i; initial begin for (i=0;i<3;i=i+1) #2; end endmodule'
probe "delay beruntun"     'module tb; logic [7:0] a; initial begin #1 a=1; #2 a=2; #3 a=3; end endmodule'
probe "delay lokal"        'module tb; initial begin logic [7:0] t; #2 t = 8'"'"'d5; end endmodule'
probe "time di display"    'module tb; initial begin $display("t=%d", $time); end endmodule'
probe "time di ekspresi"   'module tb; logic [7:0] y; initial begin y = $time + 5; end endmodule'
probe "time dua kali"      'module tb; initial begin $display("%d %d", $time, $time); end endmodule'
probe "time di perbandingan" 'module tb; logic y; initial begin if ($time > 4) y = 1'"'"'b1; else y = 1'"'"'b0; end endmodule'
probe "delay lalu finish"  'module tb; initial begin #5 $finish; end endmodule'
probe "delay satuan s"     'module tb; logic [7:0] a; initial begin #1s a = 8'"'"'d1; end endmodule'
probe "delay satuan ms"    'module tb; logic [7:0] a; initial begin #1ms a = 8'"'"'d1; end endmodule'
probe "delay satuan us"    'module tb; logic [7:0] a; initial begin #1us a = 8'"'"'d1; end endmodule'
probe "delay satuan fs"    'module tb; logic [7:0] a; initial begin #1fs a = 8'"'"'d1; end endmodule'
probe "ident bukan satuan" 'module tb; logic [7:0] step; initial begin #5 step = 8'"'"'d1; end endmodule'
probe "time tanpa sinyal"  'module tb; initial begin $display("t=%d", $time); end endmodule'
probe "time sinyal 1 bit"  'module tb; logic s; initial begin $display("t=%d", $time); end endmodule'
probe "finish lalu stmt"   'module tb; logic [7:0] a; initial begin $finish; a = 8'"'"'d99; end endmodule'
probe "delay + clock"      'module tb(input logic clk, output logic [7:0] c); always_ff @(posedge clk) c <= c + 1; initial begin c = 0; #4 c = 2; $finish; end endmodule'
probe "delay + comb"       'module tb(input [7:0] a, output [7:0] y); always_comb y = a + 1; initial begin a = 0; #4 a = 5; end endmodule'
probe "delay lalu finish"   'module tb(input logic clk, output logic [7:0] c); always_ff @(posedge clk) c <= c + 1; initial begin #4 $finish; #4 c = 1; end endmodule'
probe "always_ff body kosong" 'module tb(input logic clk, output logic q); always_ff @(posedge clk); assign q = 1'"'"'b1; endmodule'
probe "always_ff body kosong clock" 'module tb(input logic clk, output logic [7:0] c); always_ff @(posedge clk); always_comb c = 8'"'"'d7; endmodule'
probe "display %t"             'module tb; initial begin #100 $display("t=%t", $time); end endmodule'
probe "display %0t"            'module tb; initial begin #5 $display("t=%0t", $time); end endmodule'
probe "display %T kapital"     'module tb; initial begin $display("t=%T", $time); end endmodule'
probe "monitor teks"        'module tb(output logic [7:0] c); initial begin c = 0; $monitor("halo"); end endmodule'
probe "monitor dengan arg"  'module tb(output logic [7:0] c); initial begin c = 0; $monitor("c=%d", c); end endmodule'
probe "monitor tanpa kurung" 'module tb(output logic [7:0] c); initial begin c = 0; $monitor; end endmodule'
probe "monitor dua kali"    'module tb(output logic [7:0] c); initial begin c = 0; $monitor("a=%d", c); $monitor("b=%d", c); end endmodule'
probe "monitor + finish"    'module tb(output logic [7:0] c); initial begin c = 0; $monitor("c=%d", c); #4 $finish; end endmodule'
probe "monitor di loop"     'module tb(output logic [7:0] c); integer i; initial begin c = 0; for (i=0;i<2;i=i+1) $monitor("c=%d", c); end endmodule'
probe "monitoron"          'module tb(output logic [7:0] c); initial begin c = 0; $monitoron; end endmodule'
probe "monitoroff"         'module tb(output logic [7:0] c); initial begin c = 0; $monitoroff; end endmodule'
probe "monitoron lalu off"  'module tb(output logic [7:0] c); initial begin c = 0; $monitoron; #1 $monitoroff; end endmodule'
probe "monitoroff tanpa monitor" 'module tb(output logic [7:0] c); initial begin c = 0; $monitoroff; $monitoron; end endmodule'
probe "strobe teks"         'module tb(output logic [7:0] c); initial begin c = 0; $strobe("halo"); end endmodule'
probe "strobe dengan arg"   'module tb(output logic [7:0] c); initial begin c = 0; $strobe("c=%d", c); end endmodule'
probe "strobe dua format"   'module tb(output logic [7:0] c); initial begin c = 0; $strobe("a=%d", c); $strobe("b=%d", c); end endmodule'
probe "strobe + finish"     'module tb(output logic [7:0] c); initial begin c = 0; $strobe("c=%d", c); #1 $finish; end endmodule'
probe "strobe tanpa kurung" 'module tb(output logic [7:0] c); initial begin c = 0; $strobe; end endmodule'
probe "delay di instans"   'module c(input [7:0] a); initial begin #1 $display("a=%d", a); end endmodule module t2(input [7:0] z); c u(.a(z)); endmodule'

echo "--- regresi operator lama ---"
probe "a & b"            'module m(input [3:0] a, input [3:0] b, output [3:0] y); assign y = a & b; endmodule'
probe "a << b"           'module m(input [3:0] a, input [3:0] b, output [7:0] y); assign y = a << b; endmodule'
probe "a == b"           'module m(input [3:0] a, input [3:0] b, output y); assign y = a == b; endmodule'
probe "a && b"           'module m(input a, input b, output y); assign y = a && b; endmodule'
probe "case statement"   'module m(input [1:0] s, output [3:0] y); always_comb case (s) 0: y = 1; default: y = 0; endcase endmodule'
probe "always_ff nba"    'module m(input clk, input d, output q); always_ff @(posedge clk) q <= d; endmodule'
probe "hierarki"         'module c(input a, output y); assign y = ~a; endmodule module t(input x, output z); c u(.a(x), .y(z)); endmodule'
