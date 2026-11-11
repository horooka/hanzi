use std::fs;
use std::path::{Path, PathBuf};

const RESOURCE_PREFIX: &str = "/io/github/horooka/hanzi";

fn collect_files(dir: &Path, base: &Path, out: &mut Vec<String>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_files(&path, base, out);
        } else {
            let rel = path
                .strip_prefix(base)
                .expect("path under data dir")
                .to_string_lossy()
                .replace('\\', "/");
            if rel.ends_with(".gresource.xml") {
                continue;
            }
            // Keep the scalable SVG out of the GResource: some development
            // machines have no gdk-pixbuf SVG loader, and GTK prefers the
            // scalable icon over the PNG fallbacks. The SVG is still installed
            // by the Flatpak manifest, where librsvg is available.
            if rel.starts_with("icons/") && rel.ends_with(".svg") {
                continue;
            }
            out.push(rel);
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=data");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let data_dir = PathBuf::from("data");

    let mut files = Vec::new();
    collect_files(&data_dir, &data_dir, &mut files);
    files.sort();

    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gresources>\n  <gresource prefix=\"",
    );
    xml.push_str(RESOURCE_PREFIX);
    xml.push_str("\">\n");
    for file in &files {
        xml.push_str("    <file>");
        xml.push_str(file);
        xml.push_str("</file>\n");
    }
    xml.push_str("  </gresource>\n</gresources>\n");

    let xml_path = out_dir.join("hanzi.gresource.xml");
    fs::write(&xml_path, xml).expect("write gresource xml");

    glib_build_tools::compile_resources(
        &[data_dir.as_path()],
        xml_path
            .to_str()
            .expect("generated gresource path is UTF-8"),
        "hanzi.gresource",
    );
}
