fn main() {
    slint_build::compile("ui/wacu.slint").expect("failed to compile Slint UI");
    println!("cargo:rerun-if-changed=ui/wacu.slint");
    println!("cargo:rerun-if-changed=assets");
}
