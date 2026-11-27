fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("src/icon.ico");
        res.set("FileDescription", "Monitor Brightness Control");
        res.set("ProductName", "Monitor Brightness Control");
        res.set("LegalCopyright", "Copyright (c) 2026 noahdaniels");
        res.compile().expect("failed to embed Windows icon");
    }
}
