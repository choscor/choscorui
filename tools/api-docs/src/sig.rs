//! Rust signatures from rustdoc JSON types. A path shows only its last
//! segment, as the rustdoc HTML pages do, so the text does not depend on
//! where the toolkit defines an item.

use crate::json::Json;
use crate::model::{ItemRef, World};

/// Renders the declaration of an item: the text of its code block.
pub fn declaration(world: &World, item: ItemRef) -> String {
    let json = world.item(item);
    let name = json.get("name").as_str().unwrap_or_default();
    let (kind, inner) = world.inner(item);
    let mut out = String::new();
    if json
        .get("attrs")
        .items()
        .iter()
        .any(|attr| attr.as_str() == Some("non_exhaustive"))
    {
        out.push_str("#[non_exhaustive]\n");
    }
    let vis = if json.get("visibility").as_str() == Some("public") {
        "pub "
    } else {
        ""
    };
    match kind {
        "function" => out.push_str(&function(vis, name, inner)),
        "struct" => out.push_str(&structure(world, item, vis, name, inner)),
        "enum" => out.push_str(&enumeration(world, item, vis, name, inner)),
        "union" => out.push_str(&format!(
            "{vis}union {name}{} {{ /* fields */ }}",
            generics_decl(inner.get("generics"))
        )),
        "trait" => out.push_str(&trait_decl(vis, name, inner)),
        "type_alias" => {
            let generics = inner.get("generics");
            out.push_str(&format!(
                "{vis}type {name}{}{} = {};",
                generics_decl(generics),
                where_clause(generics),
                ty(inner.get("type"))
            ));
        }
        "constant" => out.push_str(&format!(
            "{vis}const {name}: {} = {};",
            ty(inner.get("type")),
            inner.get("const").get("expr").as_str().unwrap_or("_")
        )),
        "assoc_const" => {
            out.push_str(&format!("{vis}const {name}: {}", ty(inner.get("type"))));
            if let Some(value) = inner.get("value").as_str() {
                out.push_str(&format!(" = {value}"));
            }
            out.push(';');
        }
        "static" => out.push_str(&format!(
            "{vis}static {}{name}: {};",
            if inner.get("is_mutable").as_bool() {
                "mut "
            } else {
                ""
            },
            ty(inner.get("type"))
        )),
        "assoc_type" => {
            out.push_str(&format!(
                "type {name}{}",
                generics_decl(inner.get("generics"))
            ));
            let bounds = inner.get("bounds").items();
            if !bounds.is_empty() {
                out.push_str(&format!(": {}", bound_list(bounds)));
            }
            if !inner.get("type").is_null() {
                out.push_str(&format!(" = {}", ty(inner.get("type"))));
            }
            out.push(';');
        }
        "macro" => out.push_str(inner.as_str().unwrap_or_default()),
        "proc_macro" => out.push_str(&format!("{name}!")),
        _ => out.push_str(&format!("{vis}{kind} {name}")),
    }
    out
}

fn function(vis: &str, name: &str, inner: &Json) -> String {
    let header = inner.get("header");
    let mut out = String::from(vis);
    if header.get("is_const").as_bool() {
        out.push_str("const ");
    }
    if header.get("is_async").as_bool() {
        out.push_str("async ");
    }
    if header.get("is_unsafe").as_bool() {
        out.push_str("unsafe ");
    }
    let generics = inner.get("generics");
    let sig = inner.get("sig");
    out.push_str(&format!(
        "fn {name}{}({})",
        generics_decl(generics),
        inputs(sig)
    ));
    if let Some(output) = Some(sig.get("output")).filter(|o| !o.is_null()) {
        out.push_str(&format!(" -> {}", ty(output)));
    }
    out.push_str(&where_clause(generics));
    out
}

fn inputs(sig: &Json) -> String {
    let mut parts = Vec::new();
    for input in sig.get("inputs").items() {
        let name = input.items().first().and_then(Json::as_str).unwrap_or("_");
        let input_ty = input.items().get(1).unwrap_or(&Json::Null);
        parts.push(if name == "self" {
            receiver(input_ty)
        } else {
            format!("{name}: {}", ty(input_ty))
        });
    }
    if sig.get("is_c_variadic").as_bool() {
        parts.push("...".into());
    }
    parts.join(", ")
}

fn receiver(input_ty: &Json) -> String {
    let is_self = |t: &Json| t.get("generic").as_str() == Some("Self");
    if is_self(input_ty) {
        return "self".into();
    }
    let reference = input_ty.get("borrowed_ref");
    if !reference.is_null() && is_self(reference.get("type")) {
        let lifetime = reference
            .get("lifetime")
            .as_str()
            .map(|l| format!("{l} "))
            .unwrap_or_default();
        let mutability = if reference.get("is_mutable").as_bool() {
            "mut "
        } else {
            ""
        };
        return format!("&{lifetime}{mutability}self");
    }
    format!("self: {}", ty(input_ty))
}

fn structure(world: &World, item: ItemRef, vis: &str, name: &str, inner: &Json) -> String {
    let generics = inner.get("generics");
    let head = format!("{vis}struct {name}{}", generics_decl(generics));
    let where_text = where_clause(generics);
    let Some((kind, value)) = inner.get("kind").variant() else {
        return format!("{head};");
    };
    match kind {
        "unit" => format!("{head}{where_text};"),
        "tuple" => {
            let fields: Vec<String> = value
                .items()
                .iter()
                .map(|field| match world.resolve(item.krate, field) {
                    Some(field) => format!("pub {}", ty(world.inner(field).1)),
                    None => "/* private field */".to_owned(),
                })
                .collect();
            format!("{head}({}){where_text};", fields.join(", "))
        }
        _ => {
            let body = named_fields(world, item, value, "    pub ");
            format!("{head}{where_text} {{{body}}}")
        }
    }
}

/// The body of a braced struct or struct variant: the public fields, and
/// a comment for the private ones.
fn named_fields(world: &World, item: ItemRef, value: &Json, prefix: &str) -> String {
    let mut lines = Vec::new();
    for field in value.get("fields").items() {
        if let Some(field) = world.resolve(item.krate, field) {
            lines.push(format!(
                "{prefix}{}: {},",
                world.name(field),
                ty(world.inner(field).1)
            ));
        }
    }
    if value.get("has_stripped_fields").as_bool() {
        if lines.is_empty() {
            return " /* private fields */ ".to_owned();
        }
        lines.push(format!(
            "{}/* private fields */",
            " ".repeat(prefix.len() - prefix.trim_start().len())
        ));
    }
    if lines.is_empty() {
        return String::new();
    }
    format!("\n{}\n", lines.join("\n"))
}

fn enumeration(world: &World, item: ItemRef, vis: &str, name: &str, inner: &Json) -> String {
    let generics = inner.get("generics");
    let mut out = format!(
        "{vis}enum {name}{}{} {{\n",
        generics_decl(generics),
        where_clause(generics)
    );
    for variant in inner.get("variants").items() {
        if let Some(variant) = world.resolve(item.krate, variant) {
            out.push_str(&format!("    {},\n", variant_decl(world, variant)));
        }
    }
    if inner.get("has_stripped_variants").as_bool() {
        out.push_str("    /* private variants */\n");
    }
    out.push('}');
    out
}

/// One enum variant as it appears in the enum's declaration.
pub fn variant_decl(world: &World, variant: ItemRef) -> String {
    let name = world.name(variant);
    let (_, inner) = world.inner(variant);
    let mut out = name.to_owned();
    if let Some((kind, value)) = inner.get("kind").variant() {
        match kind {
            "tuple" => {
                let fields: Vec<String> = value
                    .items()
                    .iter()
                    .map(|field| match world.resolve(variant.krate, field) {
                        Some(field) => ty(world.inner(field).1),
                        None => "_".to_owned(),
                    })
                    .collect();
                out.push_str(&format!("({})", fields.join(", ")));
            }
            "struct" => {
                let fields: Vec<String> = value
                    .get("fields")
                    .items()
                    .iter()
                    .filter_map(|field| world.resolve(variant.krate, field))
                    .map(|field| format!("{}: {}", world.name(field), ty(world.inner(field).1)))
                    .collect();
                let stripped = value.get("has_stripped_fields").as_bool();
                let mut body = fields.join(", ");
                if stripped {
                    body.push_str(if body.is_empty() { ".." } else { ", .." });
                }
                out.push_str(&format!(" {{ {body} }}"));
            }
            _ => {}
        }
    }
    if let Some(expr) = inner.get("discriminant").get("expr").as_str() {
        out.push_str(&format!(" = {expr}"));
    }
    out
}

fn trait_decl(vis: &str, name: &str, inner: &Json) -> String {
    let generics = inner.get("generics");
    let mut out = String::from(vis);
    if inner.get("is_unsafe").as_bool() {
        out.push_str("unsafe ");
    }
    if inner.get("is_auto").as_bool() {
        out.push_str("auto ");
    }
    out.push_str(&format!("trait {name}{}", generics_decl(generics)));
    let bounds = inner.get("bounds").items();
    if !bounds.is_empty() {
        out.push_str(&format!(": {}", bound_list(bounds)));
    }
    out.push_str(&where_clause(generics));
    out.push_str(" { /* items below */ }");
    out
}

/// `<T: Bound, 'a>`, without the synthetic parameters of `impl Trait`
/// arguments.
fn generics_decl(generics: &Json) -> String {
    let mut parts = Vec::new();
    for param in generics.get("params").items() {
        let name = param.get("name").as_str().unwrap_or_default();
        let Some((kind, value)) = param.get("kind").variant() else {
            continue;
        };
        match kind {
            "lifetime" => {
                let outlives: Vec<&str> = value
                    .get("outlives")
                    .items()
                    .iter()
                    .filter_map(Json::as_str)
                    .collect();
                if outlives.is_empty() {
                    parts.push(name.to_owned());
                } else {
                    parts.push(format!("{name}: {}", outlives.join(" + ")));
                }
            }
            "type" => {
                if value.get("is_synthetic").as_bool() {
                    continue;
                }
                let mut part = name.to_owned();
                let bounds = value.get("bounds").items();
                if !bounds.is_empty() {
                    part.push_str(&format!(": {}", bound_list(bounds)));
                }
                if !value.get("default").is_null() {
                    part.push_str(&format!(" = {}", ty(value.get("default"))));
                }
                parts.push(part);
            }
            "const" => {
                let mut part = format!("const {name}: {}", ty(value.get("type")));
                if let Some(default) = value.get("default").as_str() {
                    part.push_str(&format!(" = {default}"));
                }
                parts.push(part);
            }
            _ => {}
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("<{}>", parts.join(", "))
    }
}

fn where_clause(generics: &Json) -> String {
    let mut parts = Vec::new();
    for predicate in generics.get("where_predicates").items() {
        let Some((kind, value)) = predicate.variant() else {
            continue;
        };
        match kind {
            "bound_predicate" => {
                let hrtb = higher_ranked(value.get("generic_params"));
                parts.push(format!(
                    "{hrtb}{}: {}",
                    ty(value.get("type")),
                    bound_list(value.get("bounds").items())
                ));
            }
            "lifetime_predicate" => {
                let outlives: Vec<&str> = value
                    .get("outlives")
                    .items()
                    .iter()
                    .filter_map(Json::as_str)
                    .collect();
                parts.push(format!(
                    "{}: {}",
                    value.get("lifetime").as_str().unwrap_or_default(),
                    outlives.join(" + ")
                ));
            }
            "eq_predicate" => {
                parts.push(format!(
                    "{} = {}",
                    ty(value.get("lhs")),
                    term(value.get("rhs"))
                ));
            }
            _ => {}
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("\nwhere\n    {},", parts.join(",\n    "))
    }
}

fn higher_ranked(params: &Json) -> String {
    let names: Vec<&str> = params
        .items()
        .iter()
        .filter_map(|p| p.get("name").as_str())
        .collect();
    if names.is_empty() {
        String::new()
    } else {
        format!("for<{}> ", names.join(", "))
    }
}

fn bound_list(bounds: &[Json]) -> String {
    bounds.iter().map(bound).collect::<Vec<_>>().join(" + ")
}

fn bound(value: &Json) -> String {
    let Some((kind, inner)) = value.variant() else {
        return "_".into();
    };
    match kind {
        "trait_bound" => {
            let modifier = match inner.get("modifier").as_str() {
                Some("maybe") => "?",
                Some("maybe_const") => "~const ",
                _ => "",
            };
            format!(
                "{}{modifier}{}",
                higher_ranked(inner.get("generic_params")),
                path(inner.get("trait"))
            )
        }
        "outlives" => inner.as_str().unwrap_or_default().to_owned(),
        "use" => {
            let args: Vec<String> = inner
                .items()
                .iter()
                .map(|arg| match arg.variant() {
                    Some((_, name)) => name.as_str().unwrap_or_default().to_owned(),
                    None => arg.as_str().unwrap_or_default().to_owned(),
                })
                .collect();
            format!("use<{}>", args.join(", "))
        }
        _ => "_".into(),
    }
}

/// A path with its generic arguments, shown by its last segment.
fn path(value: &Json) -> String {
    let full = value.get("path").as_str().unwrap_or_default();
    let name = full.rsplit("::").next().unwrap_or(full);
    format!("{name}{}", generic_args(value.get("args")))
}

fn generic_args(args: &Json) -> String {
    let Some((kind, inner)) = args.variant() else {
        return String::new();
    };
    match kind {
        "angle_bracketed" => {
            let mut parts: Vec<String> =
                inner.get("args").items().iter().map(generic_arg).collect();
            for constraint in inner.get("constraints").items() {
                let name = constraint.get("name").as_str().unwrap_or_default();
                let own = generic_args(constraint.get("args"));
                match constraint.get("binding").variant() {
                    Some(("equality", value)) => {
                        parts.push(format!("{name}{own} = {}", term(value)))
                    }
                    Some(("constraint", value)) => {
                        parts.push(format!("{name}{own}: {}", bound_list(value.items())))
                    }
                    _ => {}
                }
            }
            if parts.is_empty() {
                String::new()
            } else {
                format!("<{}>", parts.join(", "))
            }
        }
        "parenthesized" => {
            let inputs: Vec<String> = inner.get("inputs").items().iter().map(ty).collect();
            let output = inner.get("output");
            let tail = if output.is_null() {
                String::new()
            } else {
                format!(" -> {}", ty(output))
            };
            format!("({}){tail}", inputs.join(", "))
        }
        "return_type_notation" => "(..)".into(),
        _ => String::new(),
    }
}

fn generic_arg(arg: &Json) -> String {
    match arg.variant() {
        Some(("lifetime", value)) => value.as_str().unwrap_or_default().to_owned(),
        Some(("type", value)) => ty(value),
        Some(("const", value)) => value.get("expr").as_str().unwrap_or("_").to_owned(),
        _ => "_".into(),
    }
}

fn term(value: &Json) -> String {
    match value.variant() {
        Some(("type", inner)) => ty(inner),
        Some(("constant", inner)) => inner.get("expr").as_str().unwrap_or("_").to_owned(),
        _ => "_".into(),
    }
}

/// Renders one type.
pub fn ty(value: &Json) -> String {
    let Some((kind, inner)) = value.variant() else {
        return "_".into();
    };
    match kind {
        "resolved_path" => path(inner),
        "dyn_trait" => {
            let mut parts: Vec<String> = inner
                .get("traits")
                .items()
                .iter()
                .map(|poly| {
                    format!(
                        "{}{}",
                        higher_ranked(poly.get("generic_params")),
                        path(poly.get("trait"))
                    )
                })
                .collect();
            if let Some(lifetime) = inner.get("lifetime").as_str() {
                parts.push(lifetime.to_owned());
            }
            format!("dyn {}", parts.join(" + "))
        }
        "generic" | "primitive" => inner.as_str().unwrap_or_default().to_owned(),
        "function_pointer" => {
            let header = inner.get("header");
            let unsafety = if header.get("is_unsafe").as_bool() {
                "unsafe "
            } else {
                ""
            };
            let sig = inner.get("sig");
            let output = sig.get("output");
            let tail = if output.is_null() {
                String::new()
            } else {
                format!(" -> {}", ty(output))
            };
            let args: Vec<String> = sig
                .get("inputs")
                .items()
                .iter()
                .map(|input| ty(input.items().get(1).unwrap_or(&Json::Null)))
                .collect();
            format!(
                "{}{unsafety}fn({}){tail}",
                higher_ranked(inner.get("generic_params")),
                args.join(", ")
            )
        }
        "tuple" => {
            let items: Vec<String> = inner.items().iter().map(ty).collect();
            if items.len() == 1 {
                format!("({},)", items[0])
            } else {
                format!("({})", items.join(", "))
            }
        }
        "slice" => format!("[{}]", ty(inner)),
        "array" => format!(
            "[{}; {}]",
            ty(inner.get("type")),
            inner.get("len").as_str().unwrap_or("_")
        ),
        "pat" => ty(inner.get("type")),
        "impl_trait" => format!("impl {}", bound_list(inner.items())),
        "infer" => "_".into(),
        "raw_pointer" => {
            let mutability = if inner.get("is_mutable").as_bool() {
                "mut"
            } else {
                "const"
            };
            format!("*{mutability} {}", ty(inner.get("type")))
        }
        "borrowed_ref" => {
            let lifetime = inner
                .get("lifetime")
                .as_str()
                .map(|l| format!("{l} "))
                .unwrap_or_default();
            let mutability = if inner.get("is_mutable").as_bool() {
                "mut "
            } else {
                ""
            };
            format!("&{lifetime}{mutability}{}", ty(inner.get("type")))
        }
        "qualified_path" => {
            let name = inner.get("name").as_str().unwrap_or_default();
            let self_ty = ty(inner.get("self_type"));
            let own = generic_args(inner.get("args"));
            let trait_path = inner.get("trait");
            if trait_path.is_null() {
                format!("{self_ty}::{name}{own}")
            } else {
                format!("<{self_ty} as {}>::{name}{own}", path(trait_path))
            }
        }
        _ => "_".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse;

    #[test]
    fn renders_common_types() {
        let cases = [
            (
                r#"{"borrowed_ref": {"lifetime": "'_", "is_mutable": true, "type": {"resolved_path": {"path": "crate::view::Cx", "id": 1, "args": {"angle_bracketed": {"args": [{"lifetime": "'_"}], "constraints": []}}}}}}"#,
                "&'_ mut Cx<'_>",
            ),
            (
                r#"{"impl_trait": [{"trait_bound": {"trait": {"path": "FnMut", "id": 2, "args": {"parenthesized": {"inputs": [{"primitive": "u32"}], "output": null}}}, "generic_params": [], "modifier": "none"}}, {"outlives": "'static"}]}"#,
                "impl FnMut(u32) + 'static",
            ),
            (r#"{"tuple": []}"#, "()"),
            (r#"{"slice": {"generic": "T"}}"#, "[T]"),
            (
                r#"{"qualified_path": {"name": "Item", "args": null, "self_type": {"generic": "I"}, "trait": {"path": "Iterator", "id": 3, "args": null}}}"#,
                "<I as Iterator>::Item",
            ),
        ];
        for (json, expected) in cases {
            assert_eq!(ty(&parse(json).expect("valid json")), expected);
        }
    }
}
