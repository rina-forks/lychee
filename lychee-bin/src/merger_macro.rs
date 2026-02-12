// Macro for merging configuration values
macro_rules! make_merger {
    (
        for $ty:path:

        // $ty:ident { $(..$ignore:ident,)* $( $key:ident : $default:expr, )* } ) => {
        $merger_vis:vis struct $merger_struct:ident;

        $vis:vis enum $fields_enum:ident {
        $(
            $field_variant:ident $( ( $field_ty:ty ) )?
            =
            $field_name:ident
            ,
        )*
        // https://internals.rust-lang.org/t/macro-metavariables-matching-an-empty-fragment/18678/3

    }

    ) => {
        #[doc = "**Generated**. Enum with a variant for each field in the"]
        #[doc = concat!("[`", stringify!($ty), "`] struct.") ]
        #[doc = "This is useful for introspecting struct fields (e.g., when" ]
        #[doc = "determining which fields are user-defined), and this can be" ]
        #[doc = "done in a type-safe way." ]
        #[derive(Debug, Copy, Clone, Eq, PartialEq)]
        $vis enum $fields_enum {

            $(
            #[doc = "Variant for the"]
            #[doc = concat!("[`", stringify!($ty), "::", stringify!($field_name), "`]") ]
            #[doc = "field." ]
            $field_variant,
            )*
        }
        impl $fields_enum {
            /// Returns all known field names.
            $vis fn field_names() -> &'static [&'static str] {
                &[ $(stringify!($field_name), )* ]
            }
            // TODO: use this to check that field_names == clap_names == toml_names !!!!
            // this should ward off string problems as much as possible.

            /// Returns the field name for the current field variant, as a string.
            $vis fn field_name(&self) -> &'static str {
                match self {
                    $($fields_enum::$field_variant => stringify!($field_name) , )*
                }
            }

            /// Returns field variant from the given string, if possible.
            $vis fn from_field_name(s: &str) -> Result<$fields_enum, ()> {
                match s {
                    $(stringify!($field_name) => Ok($fields_enum::$field_variant), )*
                    _ => Err(())
                }
            }
        }

        $merger_vis struct $merger_struct<'a> {
            $( $( $field_name: &'a dyn Fn($field_ty, $field_ty) -> $field_ty, )? )*
            // NOTE: will break if any $field_ty is a reference, because we have no lifetimes.
        }

        impl $merger_struct<'_> {
            /// Merges the two values with overriding. Fields in `overrides`
            /// whose keys satisfy `field_is_defined` will overwrite the
            /// corresponding values from `base`.
            ///
            /// If the merger was defined with a "join" function for a particular
            /// field, then that join function will be applied
            $vis fn merge(
                &self,
                base: $ty,
                overrides: $ty,
                field_is_defined: &dyn Fn($fields_enum) -> bool,
            ) -> $ty {
                let mut base = base;
                $(

                let new = overrides.$field_name;

                if field_is_defined($fields_enum::$field_variant) {

                    $( if (true) {
                        let args = (base.$field_name, new);
                        let joiner_args: ($field_ty, $field_ty) = args; // <-- (ignore this, look at other errors first!)
                        let x = (self.$field_name)(joiner_args.0, joiner_args.1);
                        let joiner_function_result: $field_ty = x;
                        base.$field_name = joiner_function_result; // <-- type mismatch means an incorrect type was written in a `make_merger!` enum variant
                    } else )? {
                        base.$field_name = new;
                    }

                }

                )*
                base
            }
        }

        // if (false) {
        //     #[allow(dead_code, unused, clippy::diverging_sub_expression)]
        //     let _check_merge_exhaustivity = $ty {
        //         $($field_name: unreachable!(), )*
        //     };
        // };
        // $(
        //     if $cli.$key == $default && $toml.$key != $default {
        //         $cli.$key = $toml.$key;
        //     }
        // )*
    };
}

make_merger! {

    for crate::Config:

    pub(crate) struct ConfigMerger;

    pub(crate) enum ConfigField {
        Header(Vec<(String, String)>) = header,
        GithubToken = github_token,
        MaxConcurrency(usize) = max_concurrency,
    }

}

fn _f() {
    let _ = ConfigMerger {
        max_concurrency: &|a, b| a + b,
        header: &|a, b| crate::Config::merge_headers2(&a, &b),
    };
}
