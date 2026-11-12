use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=icon.png");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let ico_path = out_dir.join("icon.ico");

        if let Ok(img) = image::open("icon.png") {
            let resized = img.resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
            let _ = resized.save(&ico_path);

            let mut res = winresource::WindowsResource::new();
            res.set_icon(ico_path.to_str().unwrap());
            let _ = res.compile();
        }
    }
}
