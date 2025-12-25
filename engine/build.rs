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
}
