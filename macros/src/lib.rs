use {
    proc_macro::{self, TokenStream},
    proc_macro2::TokenStream as TokenStream2,
    quote::{quote, ToTokens},
    syn::{
        Expr, ExprLit,
        Lit, LitStr,
        parse::Parser,
        punctuated::Punctuated,
        token::Comma,
    },
};

use {
    semver::{Version, Identifier},
    std::{
        env,
        fs::read_to_string,
    },
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

#[proc_macro]
pub fn cargo_pkg_version(_: TokenStream) -> TokenStream {
    let version = Version::parse(&env::var("CARGO_PKG_VERSION").unwrap()).unwrap();
    let major = version.major;
    let minor = version.minor;
    let patch = version.patch;
    let pre = version.pre.iter().map(|x| I(x));
    let build = version.build.iter().map(|x| I(x));
    let output = quote! {
        ::semver::Version {
            major: #major,
            minor: #minor,
            patch: #patch,
            pre: vec![ #(#pre),* ],
            build: vec![ #(#build),* ],
        }
    };
    output.into()
}

struct I<'a>(&'a Identifier);

impl ToTokens for I<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let output = match self.0 {
            Identifier::Numeric(n) => quote! { ::semver::Identifier::Numeric(#n) },
            Identifier::AlphaNumeric(s) => quote! { ::semver::Identifier::AlphaNumeric(#s) },
        };
        tokens.extend(output);
    }
}
