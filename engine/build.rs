use wesl::Wesl;

fn main() {
    let wesl = Wesl::new("assets/shaders");
    wesl.build_artifact(
        &"package::passes::world_geo_draw".parse().unwrap(),
        "world_geo_draw",
    );
    wesl.build_artifact(
        &"package::passes::world_geo_generate_commands"
            .parse()
            .unwrap(),
        "world_geo_generate_commands",
    );
    wesl.build_artifact(&"package::passes::sky".parse().unwrap(), "sky");
    wesl.build_artifact(&"package::postfx::fxaa".parse().unwrap(), "postfx_fxaa");
    wesl.build_artifact(&"package::postfx::noise".parse().unwrap(), "postfx_noise");
    wesl.build_artifact(
        &"package::fullscreen_vertex".parse().unwrap(),
        "fullscreen_vertex",
    );
}
