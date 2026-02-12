use crate::options::Config;
use std::collections::HashSet;

// Macro for merging configuration values
macro_rules! make_merger {
    (
        for $ty:path:

        // $ty:ident { $(..$ignore:ident,)* $( $key:ident : $default:expr, )* } ) => {
        $merger_vis:vis struct $merger_struct:ident;

        $vis:vis enum $fields_enum:ident {
        $(
            $field_variant:ident
            =
            $field_name:ident
            $( -> $field_ty:ty )?
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
        #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
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
            $vis fn from_field_name(s: &str) -> Result<$fields_enum, &str> {
                match s {
                    $(stringify!($field_name) => Ok($fields_enum::$field_variant), )*
                    s => Err(s)
                }
            }
        }

        $merger_vis struct $merger_struct {
            $( $( $field_name: &dyn Fn($field_ty, $field_ty) -> $field_ty, )? )*
            // NOTE: will break if any $field_ty is a reference, because we have no lifetimes.
        }

        impl $merger_struct {
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
                $ty {
                $(
                $field_name: if field_is_defined($fields_enum::$field_variant) {
                    $( if (true) {
                        let args = (base.$field_name, overrides.$field_name);
                        let joiner_args: ($field_ty, $field_ty) = args; // <-- (ignore this, look at other errors first!)
                        let x = (self.$field_name)(joiner_args.0, joiner_args.1);
                        let joiner_function_result: $field_ty = x;
                        joiner_function_result // <-- type mismatch means an incorrect type was written in a `make_merger!` enum variant
                    } else )? {
                        overrides.$field_name
                    }
                } else {
                    base.$field_name
                },

                )*
                }
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
        // Header = header -> Vec<(String, String)>,
        // GithubToken = github_token,
        // MaxConcurrency = max_concurrency -> usize,

Accept = accept,
Archive = archive,
Base = base,
BaseUrl = base_url,
BasicAuth = basic_auth,
Cache = cache,
CacheExcludeStatus = cache_exclude_status,
CookieJar = cookie_jar,
DefaultExtension = default_extension,
Dump = dump,
DumpInputs = dump_inputs,
Exclude = exclude,
ExcludeAllPrivate = exclude_all_private,
ExcludeFile = exclude_file,
ExcludeLinkLocal = exclude_link_local,
ExcludeLoopback = exclude_loopback,
ExcludePath = exclude_path,
ExcludePrivate = exclude_private,
Extensions = extensions,
FallbackExtensions = fallback_extensions,
FilesFrom = files_from,
Format = format,
Generate = generate,
GithubToken = github_token,
GlobIgnoreCase = glob_ignore_case,
Header = header,
Hidden = hidden,
Hosts = hosts,
HostConcurrency = host_concurrency,
HostRequestInterval = host_request_interval,
HostStats = host_stats,
Include = include,
IncludeFragments = include_fragments,
IncludeMail = include_mail,
IncludeVerbatim = include_verbatim,
IncludeWikilinks = include_wikilinks,
IndexFiles = index_files,
Insecure = insecure,
MaxCacheAge = max_cache_age,
MaxConcurrency = max_concurrency,
MaxRedirects = max_redirects,
MaxRetries = max_retries,
Method = method,
MinTls = min_tls,
Mode = mode,
NoIgnore = no_ignore,
NoProgress = no_progress,
Offline = offline,
Output = output,
Preprocess = preprocess,
Remap = remap,
RequireHttps = require_https,
RetryWaitTime = retry_wait_time,
RootDir = root_dir,
Scheme = scheme,
SkipMissing = skip_missing,
Suggest = suggest,
Threads = threads,
Timeout = timeout,
UserAgent = user_agent,
Verbose = verbose,
    }

}

fn _f() {
    let _ = ConfigMerger {
        // max_concurrency: &|a, b| a + b,
        // header: &|a, b| crate::Config::merge_headers2(&a, &b),
    };
}

pub(crate) fn all_toml_names() -> &'static [&'static str] {
    serde_aux::serde_introspection::serde_introspect::<Config>()
}

pub(crate) fn all_clap_args() -> Vec<clap::Id> {
    <Config as clap::CommandFactory>::command()
        .get_arguments()
        .map(|arg| arg.get_id().clone())
        .collect()
}

pub(crate) fn toml_name_to_field(x: &str) -> Option<ConfigField> {
    ConfigField::from_field_name(x).ok()
}

pub(crate) fn clap_arg_to_field(x: &clap::Id) -> Option<ConfigField> {
    match x.as_str() {
        "quiet" => Some(ConfigField::Verbose),
        s => ConfigField::from_field_name(s).ok(),
    }
}

pub(crate) fn merge(x: Config, other: Config, defined_set: &HashSet<ConfigField>) -> Config {
    println!("defined: {:?}", defined_set);
    let is_defined = |x| defined_set.contains(&x);
    ConfigMerger {}.merge(x, other, &is_defined)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_toml_name() {
        for x in all_toml_names() {
            assert!(toml_name_to_field(x).is_some());
        }
        for x in all_clap_args() {
            assert!(clap_arg_to_field(x).is_some());
        }
    }
}
