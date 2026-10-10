// The CLAP entry point (`clap_entry`) comes from the Rust static library, which is force-loaded
// into this module; clap-wrapper finds it in the component's own image at runtime.
extern "C" int jovian8_auv2_anchor() { return 0; }
