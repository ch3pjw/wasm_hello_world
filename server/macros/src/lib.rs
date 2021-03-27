use {
    proc_macro::{self, TokenStream},
    quote::quote,
    syn::parse_macro_input,
};

#[proc_macro]
pub fn hello(input: TokenStream) -> TokenStream {
    let output = quote! {
        println!("hello world")
    };
    output.into()
}
