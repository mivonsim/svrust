// Tanggung jawab: integration test LHS bergenvar pada badan task (LRM §13.3).
mod support;

use support::build;
use sv_ir::{Assignment, Statement};

/// Kumpulkan seluruh assignment di dalam pohon statement.
fn kumpulkan_assign<'a>(stmt: &'a Statement, hasil: &mut Vec<&'a Assignment>) {
    match stmt {
        Statement::Assign { assignment, .. } => hasil.push(assignment),
        Statement::Block { body, .. } => {
            for s in body {
                kumpulkan_assign(s, hasil);
            }
        }
        Statement::If {
            then_branch,
            else_branch,
            ..
        } => {
            for s in then_branch.iter().chain(else_branch) {
                kumpulkan_assign(s, hasil);
            }
        }
        Statement::Case { arms, .. } => {
            for arm in arms {
                for s in &arm.body {
                    kumpulkan_assign(s, hasil);
                }
            }
        }
        Statement::For { init, body, .. } => {
            hasil.push(init);
            for s in body {
                kumpulkan_assign(s, hasil);
            }
        }
        Statement::Repeat { body, .. } | Statement::While { body, .. } => {
            for s in body {
                kumpulkan_assign(s, hasil);
            }
        }
        _ => {}
    }
}

/// Selektor `1'b1` konstan pada badan task.
fn konstan_di_proses(proses: &sv_ir::Process) -> Vec<u64> {
    let mut hasil = Vec::new();
    for stmt in &proses.body {
        let mut semua = Vec::new();
        kumpulkan_assign(stmt, &mut semua);
        for a in semua {
            if let sv_ir::Expr::Const { value, .. } = a.value {
                hasil.push(value);
            }
        }
    }
    hasil
}

/// BUG: `ganti_lvalue` memaksa `genvar_index: None` saat menukar formal `q`
/// menjadi sinyal nyata `r`. LHS `q[i]` jadi `r` polos sehingga SELURUH
/// sinyal ditulis, bukan satu bit. Hasil simulasi: `r = 00000001` untuk
/// iterasi `i = 1,2,3`, padahal seharusnya `00001110`.
#[test]
fn lhs_genvar_pada_badan_task_mempertahankan_indeks() {
    let src = "module m;\n\
               logic [7:0] r;\n\
               genvar i;\n\
               task automatic set_bit(output logic [7:0] q);\n\
                 begin q[i] = 1'b1; end\n\
               endtask\n\
               generate\n\
                 for (i = 1; i < 4; i = i + 1) begin : g\n\
                   always_comb set_bit(r);\n\
                 end\n\
               endgenerate\n\
             endmodule";
    let design = build(src);
    let proses: Vec<_> = design
        .processes
        .iter()
        .filter(|p| !p.body.is_empty())
        .collect();
    assert_eq!(proses.len(), 3, "loop generate harus menghasilkan 3 proses");

    for p in &proses {
        let mut semua = Vec::new();
        for stmt in &p.body {
            kumpulkan_assign(stmt, &mut semua);
        }
        assert!(
            semua.iter().any(|a| a.slice.is_some()),
            "proses {:?} menulis tanpa slice: seluruh sinyal akan tertimpa",
            p.name
        );
    }
    let total: Vec<u64> = proses.iter().flat_map(|p| konstan_di_proses(p)).collect();
    assert_eq!(total, vec![1, 1, 1], "semua iterasi menulis 1'b1");
}

/// BUG yang sama, tapi slice konstan: `q[3:0] = x` harus tetap terpotong
/// setelah formal ditukar.
#[test]
fn slice_pada_badan_task_ikut_terbawa() {
    let src = "module m;\n\
               logic [7:0] r;\n\
               task automatic isi_bawah(output logic [7:0] q);\n\
                 begin q[3:0] = 8'h0F; end\n\
               endtask\n\
               always_comb isi_bawah(r);\n\
             endmodule";
    let design = build(src);
    let mut semua = Vec::new();
    for stmt in &design.processes[0].body {
        kumpulkan_assign(stmt, &mut semua);
    }
    assert_eq!(semua.len(), 1, "harus ada satu assignment");
    assert!(
        semua[0].slice.is_some(),
        "slice harus ikut dibawa ke sinyal nyata"
    );
}
