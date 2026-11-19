// Intègre l'icône dans le .exe (Windows uniquement).
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Kimbo - Placement");
        res.set("FileDescription", "Assistant de placement - Canopée du Kimbo");
        if let Err(e) = res.compile() {
            eprintln!("Impossible d'intégrer l'icône : {e}");
            std::process::exit(1);
        }
    }
}
