// Intègre l'icône dans le .exe (Windows uniquement).
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Assistant donjons");
        res.set("FileDescription", "Assistant donjons - Kimbo et Tengu Givrefoux");
        if let Err(e) = res.compile() {
            eprintln!("Impossible d'intégrer l'icône : {e}");
            std::process::exit(1);
        }
    }
}
