fn main() {
    embed_resource::compile("assets/wacu.rc", embed_resource::NONE)
        .manifest_required()
        .expect("failed to embed Wacu application icon");
    slint_build::compile("ui/wacu.slint").expect("failed to compile Slint UI");
    println!("cargo:rerun-if-changed=ui/wacu.slint");
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=docs/logo.png");
}
