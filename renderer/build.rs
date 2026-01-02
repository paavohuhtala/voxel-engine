use wesl::{CompileOptions, Wesl};

fn main() {
    let mut wesl = Wesl::new("assets/shaders");

    wesl.set_options(CompileOptions {
        lazy: false,
        ..Default::default()
    });

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
