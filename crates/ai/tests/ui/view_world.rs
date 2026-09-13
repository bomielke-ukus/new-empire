use ai_api::FoggedView;
fn peek(view: FoggedView<'_>) {
    let _ = view.world();
}
fn main() {}
