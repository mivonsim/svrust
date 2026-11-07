// Tanggung jawab: pemetaan keyword SystemVerilog ke token.
use crate::token::Token;

pub fn keyword_or_ident(s: &str) -> Token {
    match s {
        "module" => Token::Module,
        "endmodule" => Token::Endmodule,
        "logic" => Token::Logic,
        "wire" => Token::Wire,
        "reg" => Token::Reg,
        "bit" => Token::Bit,
        // LRM §6.14: `string` — tipe string dinamis.
        "string" => Token::String,
        "integer" => Token::Integer,
        "int" => Token::Int,
        "byte" => Token::Byte,
        "shortint" => Token::ShortInt,
        "longint" => Token::LongInt,
        "time" => Token::Time,
        "real" => Token::Real,
        "supply0" => Token::Supply0,
        "supply1" => Token::Supply1,
        "struct" => Token::Struct,
        "union" => Token::Union,
        // LRM §6.6: `tri` = wired-OR net, `wor` = wired-OR net, `wand` = wired-AND net.
        "tri" => Token::Tri,
        "wor" => Token::Wor,
        "wand" => Token::Wand,
        // LRM §25.6: `bind` — binding antarmuka ke instance modul. SVRust
        // melewati statement `bind ... ;` di isi modul.
        "bind" => Token::Bind,
        // LRM §8: `class` / `endclass` — deklarasi kelas OOP. SVRust melewati
        // isi kelas di level berkas; hanya pasangan kata kunci yang dibutuhkan
        // agar parser tidak error pada berkas verifikasi (UVM/DV).
        "class" => Token::Class,
        "endclass" => Token::Endclass,
        "assert" => Token::Assert,
        // `package` — deklarasi paket yang berisi definisi tipe, parameter, dan subroutine (LRM §26).
        "package" => Token::Package,
        // `endpackage` — penutup deklarasi paket (LRM §26).
        "endpackage" => Token::Endpackage,
        // `property` — deklarasi properti SVA concurrent assertion (LRM §20/§21).
        "property" => Token::Property,
        "while" => Token::While,
        "repeat" => Token::Repeat,
        // `forever` — loop tak berujung (LRM §12.8).
        "forever" => Token::Forever,
        // LRM §13.4: `void` — function tanpa nilai balik.
        "void" => Token::Void,
        // LRM §12.13: `final` — blok penutup eksekusi akhir simulasi.
        "final" => Token::Final,
        "initial" => Token::Initial,
        "signed" => Token::Signed,
        "unsigned" => Token::Unsigned,
        "input" => Token::Input,
        "output" => Token::Output,
        "inout" => Token::Inout,
        "parameter" => Token::Parameter,
        "localparam" => Token::LocalParam,
        "assign" => Token::Assign,
        // `deassign` — melepasi penetapans berkelanjutan prosedural (LRM §14).
        "deassign" => Token::Deassign,
        "always" => Token::Always,
        "always_comb" => Token::AlwaysComb,
        "always_ff" => Token::AlwaysFf,
        "always_latch" => Token::AlwaysLatch,
        "posedge" => Token::Posedge,
        "negedge" => Token::Negedge,
        // LRM §9.7: `edge` memicu event control pada perubahan apa pun.
        "edge" => Token::Edge,
        // LRM §9.7: `or` memisahkan item pada event control gabungan.
        "or" => Token::Or,
        "if" => Token::If,
        "else" => Token::Else,
        "for" => Token::For,
        "case" => Token::Case,
        "casez" => Token::Casez,
        "casex" => Token::Casex,
        "endcase" => Token::Endcase,
        "default" => Token::Default,
        "begin" => Token::Begin,
        "end" => Token::End,
        // LRM §27: region generate dan penghitung `genvar`.
        "generate" => Token::Generate,
        "endgenerate" => Token::Endgenerate,
        "genvar" => Token::Genvar,
        // LRM §13.3/§13.4: subroutine task dan function.
        "task" => Token::Task,
        "endtask" => Token::Endtask,
        "function" => Token::Function,
        "endfunction" => Token::Endfunction,
        "automatic" => Token::Automatic,
        "return" => Token::Return,
        // LRM §8.20 `typedef` dan LRM §6.7 `enum`.
        "typedef" => Token::Typedef,
        "enum" => Token::Enum,
        _ => Token::Ident(s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_dipetakan_sebagai_keyword() {
        assert_eq!(keyword_or_ident("edge"), Token::Edge);
    }

    #[test]
    fn or_dipetakan_sebagai_keyword() {
        // LRM §9.7: pemisah item pada event control gabungan.
        assert_eq!(keyword_or_ident("or"), Token::Or);
    }

    #[test]
    fn keyword_terpetakan() {
        assert_eq!(keyword_or_ident("module"), Token::Module);
        assert_eq!(keyword_or_ident("endmodule"), Token::Endmodule);
        assert_eq!(keyword_or_ident("always_ff"), Token::AlwaysFf);
    }

    #[test]
    fn ident_tidak_dikenal() {
        assert_eq!(keyword_or_ident("foo"), Token::Ident("foo".to_string()));
    }

    #[test]
    fn signed_terpetakan_sebagai_keyword() {
        assert_eq!(keyword_or_ident("signed"), Token::Signed);
        assert_eq!(keyword_or_ident("unsigned"), Token::Unsigned);
    }

    #[test]
    fn casez_casex_terpetakan() {
        assert_eq!(keyword_or_ident("casez"), Token::Casez);
        assert_eq!(keyword_or_ident("casex"), Token::Casex);
    }

    #[test]
    fn for_terpetakan() {
        assert_eq!(keyword_or_ident("for"), Token::For);
    }

    // LRM §27: keyword region generate.
    #[test]
    fn generate_terpetakan() {
        assert_eq!(keyword_or_ident("generate"), Token::Generate);
        assert_eq!(keyword_or_ident("endgenerate"), Token::Endgenerate);
        assert_eq!(keyword_or_ident("genvar"), Token::Genvar);
    }

    // LRM §13.3/§13.4: keyword subroutine.
    #[test]
    fn task_function_terpetakan_sebagai_keyword() {
        assert_eq!(keyword_or_ident("task"), Token::Task);
        assert_eq!(keyword_or_ident("endtask"), Token::Endtask);
        assert_eq!(keyword_or_ident("function"), Token::Function);
        assert_eq!(keyword_or_ident("endfunction"), Token::Endfunction);
    }

    #[test]
    fn automatic_return_terpetakan() {
        assert_eq!(keyword_or_ident("automatic"), Token::Automatic);
        assert_eq!(keyword_or_ident("return"), Token::Return);
    }

    // LRM §8.20 `typedef` dan LRM §6.7 `enum`.
    #[test]
    fn typedef_enum_terpetakan_sebagai_keyword() {
        assert_eq!(keyword_or_ident("typedef"), Token::Typedef);
        assert_eq!(keyword_or_ident("enum"), Token::Enum);
    }

    #[test]
    fn property_terpetakan_sebagai_keyword() {
        assert_eq!(keyword_or_ident("property"), Token::Property);
    }

    // LRM §6.14: `string` adalah tipe string dinamis, bukan identifier bebas.
    #[test]
    fn string_terpetakan_sebagai_keyword() {
        assert_eq!(keyword_or_ident("string"), Token::String);
    }

    // LRM §8: `class` / `endclass` adalah kata kunci OOP, bukan identifier bebas.
    #[test]
    fn class_endclass_terpetakan_sebagai_keyword() {
        assert_eq!(keyword_or_ident("class"), Token::Class);
        assert_eq!(keyword_or_ident("endclass"), Token::Endclass);
    }

    // LRM §25.6: `bind` adalah kata kunci binding, bukan identifier bebas.
    #[test]
    fn bind_terpetakan_sebagai_keyword() {
        assert_eq!(keyword_or_ident("bind"), Token::Bind);
    }
}
