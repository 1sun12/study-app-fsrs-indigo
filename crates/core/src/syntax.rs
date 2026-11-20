//! Known-language table (SPEC §5, D-12): a category named like a language gets highlighting.
//! Values are Shiki language ids; `src/lib/syntax.test.ts` checks each one exists.

const TABLE: &[(&str, &str)] = &[
    ("python", "python"),
    ("py", "python"),
    ("gdscript", "gdscript"),
    ("gd", "gdscript"),
    ("godot shaders", "gdshader"),
    ("gdshader", "gdshader"),
    ("c", "c"),
    ("cpp", "cpp"),
    ("c++", "cpp"),
    ("rust", "rust"),
    ("lua", "lua"),
    ("javascript", "javascript"),
    ("js", "javascript"),
    ("typescript", "typescript"),
    ("ts", "typescript"),
    ("jsx", "jsx"),
    ("tsx", "tsx"),
    ("html", "html"),
    ("css", "css"),
    ("scss", "scss"),
    ("less", "less"),
    ("sql", "sql"),
    ("bash", "bash"),
    ("shell", "bash"),
    ("sh", "bash"),
    ("powershell", "powershell"),
    ("glsl", "glsl"),
    ("hlsl", "hlsl"),
    ("wgsl", "wgsl"),
    ("json", "json"),
    ("toml", "toml"),
    ("yaml", "yaml"),
    ("xml", "xml"),
    ("markdown", "markdown"),
    ("md", "markdown"),
    ("latex", "latex"),
    ("go", "go"),
    ("golang", "go"),
    ("java", "java"),
    ("kotlin", "kotlin"),
    ("groovy", "groovy"),
    ("scala", "scala"),
    ("csharp", "csharp"),
    ("c#", "csharp"),
    ("fsharp", "fsharp"),
    ("f#", "fsharp"),
    ("visual basic", "vb"),
    ("vb", "vb"),
    ("swift", "swift"),
    ("objective-c", "objective-c"),
    ("dart", "dart"),
    ("ruby", "ruby"),
    ("php", "php"),
    ("perl", "perl"),
    ("r", "r"),
    ("julia", "julia"),
    ("matlab", "matlab"),
    ("fortran", "fortran-free-form"),
    ("haskell", "haskell"),
    ("ocaml", "ocaml"),
    ("elm", "elm"),
    ("elixir", "elixir"),
    ("erlang", "erlang"),
    ("clojure", "clojure"),
    ("lisp", "common-lisp"),
    ("common lisp", "common-lisp"),
    ("scheme", "scheme"),
    ("racket", "racket"),
    ("prolog", "prolog"),
    ("zig", "zig"),
    ("nim", "nim"),
    ("d", "d"),
    ("assembly", "asm"),
    ("asm", "asm"),
    ("solidity", "solidity"),
    ("svelte", "svelte"),
    ("vue", "vue"),
    ("graphql", "graphql"),
    ("dockerfile", "dockerfile"),
    ("docker", "dockerfile"),
    ("makefile", "makefile"),
    ("make", "makefile"),
    ("cmake", "cmake"),
    ("nix", "nix"),
    ("terraform", "terraform"),
    ("hcl", "hcl"),
];

/// Syntax id for a category name, case-insensitive, or `None` for a tool-like name.
pub fn known_syntax(name: &str) -> Option<&'static str> {
    let n = name.trim().to_ascii_lowercase();
    TABLE.iter().find(|(k, _)| *k == n).map(|(_, v)| *v)
}

#[cfg(test)]
mod tests {
    use super::known_syntax;

    #[test]
    fn matches_case_insensitively_and_ignores_tools() {
        assert_eq!(known_syntax("Python"), Some("python"));
        assert_eq!(known_syntax(" GDScript "), Some("gdscript"));
        assert_eq!(known_syntax("C++"), Some("cpp"));
        assert_eq!(known_syntax("Blender"), None);
    }
}
