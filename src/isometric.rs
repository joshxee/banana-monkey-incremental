//! Lo-fi isometric village presentation.
//!
//! This module owns only drawing. The economy continues to use distances,
//! work and segment boundaries from `domain`; the projected board is a view of
//! that state, never an input to it.

use bevy::prelude::*;

use crate::game::{BROWN, CREAM};

pub(crate) const BOARD_SKY: Color = Color::srgb(0.83, 0.93, 0.84);
const GROUND: Color = Color::srgb(0.61, 0.76, 0.43);
const PATH: Color = Color::srgb(0.84, 0.73, 0.51);
const JUNGLE_TOP: Color = Color::srgb(0.24, 0.55, 0.24);
const JUNGLE_LEFT: Color = Color::srgb(0.14, 0.38, 0.17);
const JUNGLE_RIGHT: Color = Color::srgb(0.19, 0.46, 0.19);
const BLUE_TOP: Color = Color::srgb(0.29, 0.64, 0.78);
const BLUE_LEFT: Color = Color::srgb(0.18, 0.42, 0.58);
const BLUE_RIGHT: Color = Color::srgb(0.23, 0.52, 0.68);
const CORAL_TOP: Color = Color::srgb(0.92, 0.55, 0.43);
const CORAL_LEFT: Color = Color::srgb(0.69, 0.32, 0.27);
const CORAL_RIGHT: Color = Color::srgb(0.81, 0.42, 0.34);
const GOLD_TOP: Color = Color::srgb(0.96, 0.77, 0.27);
const GOLD_LEFT: Color = Color::srgb(0.72, 0.49, 0.13);
const GOLD_RIGHT: Color = Color::srgb(0.85, 0.61, 0.18);
const PURPLE_TOP: Color = Color::srgb(0.63, 0.49, 0.72);
const PURPLE_LEFT: Color = Color::srgb(0.42, 0.31, 0.53);
const PURPLE_RIGHT: Color = Color::srgb(0.52, 0.39, 0.63);

const JUNGLE_CUBES: [(f32, f32, f32, f32); 9] = [
    (-0.42, 0.21, 0.115, 0.19),
    (-0.27, 0.20, 0.095, 0.16),
    (-0.15, 0.25, 0.105, 0.20),
    (0.03, 0.29, 0.100, 0.18),
    (0.17, 0.24, 0.115, 0.22),
    (0.31, 0.16, 0.105, 0.19),
    (0.40, 0.07, 0.095, 0.17),
    (-0.43, -0.02, 0.085, 0.15),
    (0.43, -0.08, 0.085, 0.15),
];

#[derive(Component)]
pub(crate) struct WorldRoot;

#[derive(Clone)]
struct FaceMaterials {
    top: Handle<ColorMaterial>,
    left: Handle<ColorMaterial>,
    right: Handle<ColorMaterial>,
}

impl FaceMaterials {
    fn new(materials: &mut Assets<ColorMaterial>, top: Color, left: Color, right: Color) -> Self {
        Self {
            top: materials.add(top),
            left: materials.add(left),
            right: materials.add(right),
        }
    }
}

pub(crate) fn spawn_world(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    let ground = materials.add(GROUND);
    let path = materials.add(PATH);
    let jungle = FaceMaterials::new(materials, JUNGLE_TOP, JUNGLE_LEFT, JUNGLE_RIGHT);
    let blue = FaceMaterials::new(materials, BLUE_TOP, BLUE_LEFT, BLUE_RIGHT);
    let coral = FaceMaterials::new(materials, CORAL_TOP, CORAL_LEFT, CORAL_RIGHT);
    let gold = FaceMaterials::new(materials, GOLD_TOP, GOLD_LEFT, GOLD_RIGHT);
    let purple = FaceMaterials::new(materials, PURPLE_TOP, PURPLE_LEFT, PURPLE_RIGHT);

    commands
        .spawn((WorldRoot, Transform::default(), Visibility::default()))
        .with_children(|root| {
            root.spawn((
                Sprite::from_color(BOARD_SKY, Vec2::ONE),
                Transform::from_xyz(0.0, 0.0, -20.0),
            ));
            for (size, position) in [
                (Vec2::new(1.0, 0.007), Vec2::new(0.0, 0.4965)),
                (Vec2::new(1.0, 0.007), Vec2::new(0.0, -0.4965)),
                (Vec2::new(0.007, 1.0), Vec2::new(-0.4965, 0.0)),
                (Vec2::new(0.007, 1.0), Vec2::new(0.4965, 0.0)),
            ] {
                root.spawn((
                    Sprite::from_color(BROWN, size),
                    Transform::from_xyz(position.x, position.y, 2.8),
                ));
            }

            // A wide ground diamond fills the square while leaving a quiet sky
            // margin above it for the tall jungle silhouettes.
            spawn_diamond(
                root,
                meshes,
                &ground,
                Vec2::new(0.0, -0.03),
                Vec2::new(0.92, 0.48),
                -10.0,
            );

            // Two intersecting paths establish the projection and reserve a
            // clear corridor for the worker cycle.
            spawn_diamond(
                root,
                meshes,
                &path,
                Vec2::new(0.0, -0.055),
                Vec2::new(0.70, 0.105),
                -8.0,
            );
            spawn_diamond(
                root,
                meshes,
                &path,
                Vec2::new(0.04, -0.03),
                Vec2::new(0.13, 0.44),
                -8.0,
            );

            // Jungle stays around the back and edges. Its tall silhouette is
            // intentionally absent from the central route corridor.
            for (x, y, width, height) in JUNGLE_CUBES {
                spawn_cube(root, meshes, &jungle, Vec2::new(x, y), width, height);
            }

            // A tiny village of deliberately different masses. Shape and
            // scale distinguish the roles even without final isometric art.
            for (position, width, height, palette) in [
                (Vec2::new(-0.18, -0.08), 0.16, 0.11, &blue),
                (Vec2::new(0.02, 0.02), 0.12, 0.18, &gold),
                (Vec2::new(0.19, -0.09), 0.19, 0.10, &coral),
                (Vec2::new(-0.01, -0.22), 0.14, 0.09, &purple),
                (Vec2::new(0.27, -0.25), 0.11, 0.15, &blue),
                (Vec2::new(-0.29, -0.23), 0.10, 0.08, &gold),
            ] {
                spawn_cube(root, meshes, palette, position, width, height);
            }

            // Brown posts make the two economy endpoints legible without
            // pretending the lo-fi blocks are finished art.
            for (x, y) in [(-0.315, 0.060), (0.315, -0.130)] {
                root.spawn((
                    Sprite::from_color(BROWN, Vec2::new(0.008, 0.09)),
                    Transform::from_xyz(x, y - 0.035, 3.0),
                ));
                root.spawn((
                    Sprite::from_color(CREAM, Vec2::new(0.105, 0.035)),
                    Transform::from_xyz(x, y + 0.020, 3.1),
                ));
            }
        });
}

fn spawn_diamond(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    material: &Handle<ColorMaterial>,
    center: Vec2,
    size: Vec2,
    z: f32,
) {
    let left = center + Vec2::new(-size.x * 0.5, 0.0);
    let top = center + Vec2::new(0.0, size.y * 0.5);
    let right = center + Vec2::new(size.x * 0.5, 0.0);
    let bottom = center + Vec2::new(0.0, -size.y * 0.5);
    spawn_triangle(parent, meshes, material, [left, top, right], z);
    spawn_triangle(parent, meshes, material, [left, right, bottom], z);
}

fn spawn_cube(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    palette: &FaceMaterials,
    center: Vec2,
    width: f32,
    height: f32,
) {
    let depth = width * 0.48;
    let top = center + Vec2::new(0.0, depth * 0.5);
    let right = center + Vec2::new(width * 0.5, 0.0);
    let bottom = center + Vec2::new(0.0, -depth * 0.5);
    let left = center + Vec2::new(-width * 0.5, 0.0);
    let down = Vec2::new(0.0, -height);
    // Higher projected objects are further away. The stable centre-derived
    // band makes front blocks cover back blocks without per-frame sorting.
    let z = 1.0 - (center.y - height);

    spawn_triangle(parent, meshes, &palette.top, [left, top, right], z + 0.03);
    spawn_triangle(
        parent,
        meshes,
        &palette.top,
        [left, right, bottom],
        z + 0.03,
    );
    spawn_triangle(
        parent,
        meshes,
        &palette.left,
        [left, bottom, bottom + down],
        z + 0.02,
    );
    spawn_triangle(
        parent,
        meshes,
        &palette.left,
        [left, bottom + down, left + down],
        z + 0.02,
    );
    spawn_triangle(
        parent,
        meshes,
        &palette.right,
        [bottom, right, right + down],
        z + 0.01,
    );
    spawn_triangle(
        parent,
        meshes,
        &palette.right,
        [bottom, right + down, bottom + down],
        z + 0.01,
    );
}

fn spawn_triangle(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    material: &Handle<ColorMaterial>,
    points: [Vec2; 3],
    z: f32,
) {
    parent.spawn((
        Mesh2d(meshes.add(Triangle2d::new(points[0], points[1], points[2]))),
        MeshMaterial2d(material.clone()),
        Transform::from_xyz(0.0, 0.0, z),
    ));
}

#[cfg(test)]
mod tests {
    use super::JUNGLE_CUBES;

    #[test]
    fn jungle_keeps_the_route_corridor_clear() {
        let route_half_width = 0.075;
        for (x, y, _, _) in JUNGLE_CUBES {
            // The main route slopes from upper-left to lower-right.
            let route_y = -0.42 * x - 0.055;
            assert!((y - route_y).abs() > route_half_width);
        }
    }
}
