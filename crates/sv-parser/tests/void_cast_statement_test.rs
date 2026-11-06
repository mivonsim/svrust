// Tanggung jawab: menguji parsing ekspresi cast-to-void sebagai statement.
use sv_ast::combinational::CombinationalStatement;
use sv_ast::statement::Statement;

#[test]
fn void_cast_diterima_sebagai_expression_statement() {
    let tokens = sv_lexer::lex("module m; initial void'(f()); endmodule").expect("lex");
    let module = sv_parser::parse_module(&tokens).expect("parse");

    let Statement::Initial { body, .. } = &module.statements[0] else {
        panic!("statement harus initial");
    };
    let CombinationalStatement::ExprStmt { expr, .. } = &body[0] else {
        panic!("body harus expression statement");
    };
    assert!(matches!(
        expr.as_ref(),
        sv_ast::expression::Expr::Cast { type_name, .. } if type_name == "void"
    ));
}
