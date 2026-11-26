//! Wraps each demo file in the function that draws it, written out as text into OUT_DIR, so the
//! demo's `window` and `cx` are that function's parameters. (Declared in a `macro_rules!`
//! expansion instead, hygiene would hide them from the demo's code.)

use std::fs;
use std::path::Path;

fn main() {
    let demos = Path::new(env!("CARGO_MANIFEST_DIR")).join("../demos");
    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR is set")).join("demos");
    println!("cargo:rerun-if-changed={}", demos.display());
    for component in fs::read_dir(&demos).expect("docs-site/demos is readable") {
        let component = component.expect("a demos folder entry").path();
        if !component.is_dir() {
            continue;
        }
        let name = component.file_name().expect("a folder name");
        fs::create_dir_all(out.join(name)).expect("OUT_DIR is writable");
        for file in fs::read_dir(&component).expect("a component's folder is readable") {
            let file = file.expect("a demo file").path();
            if file.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let code = fs::read_to_string(&file).expect("a demo file is readable");
            let wrapped = format!(
                "{{\n\
                 #[allow(unused_variables)]\n\
                 fn render(window: &mut Window, cx: &mut App) -> AnyElement {{\n\
                 #[allow(unused_imports)]\n\
                 use gpui::*;\n\
                 #[allow(unused_imports)]\n\
                 use gpui_core::*;\n\
                 let element = {{\n{code}\n}};\n\
                 element.into_any_element()\n\
                 }}\n\
                 render\n\
                 }}\n"
            );
            let name = file.file_name().expect("a file name");
            fs::write(out.join(component.file_name().unwrap()).join(name), wrapped)
                .expect("OUT_DIR is writable");
        }
    }
}
