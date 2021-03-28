use {
    proc_macro::{self, TokenStream},
    quote::quote,
    syn::{
        Expr, ExprLit,
        Lit, LitStr,
        parse::Parser,
        punctuated::Punctuated,
        token::Comma,
    },
};

use {
    std::env,
    std::fs::read_to_string,
};

#[proc_macro]
pub fn template(input: TokenStream) -> TokenStream {
    let args = Punctuated::<Expr, Comma>::parse_separated_nonempty.parse(input).unwrap();
    let path_lit = match &args[0] {
        Expr::Lit(ExprLit{ lit: Lit::Str(s), .. }) => s,
        _ => panic!("first argument must be a path")
    };
    let args = &args.iter().collect::<Vec<&Expr>>()[1..];
    let path = env::current_dir().unwrap().join("src/").join(path_lit.value());
    let s_lit = LitStr::new(&read_to_string(path).unwrap(), path_lit.span());
    let output = quote! { format!(#s_lit, #(#args),*) };
    output.into()
}
