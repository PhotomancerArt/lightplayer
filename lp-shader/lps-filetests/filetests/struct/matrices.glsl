// test run
//
// GLSL matrix layout (column-major):
//   m[i] is column i; m[i][j] is row j of that column
//   mat2(1,2,3,4): col0=(1,2), col1=(3,4) => m[1][0]==3, m[0][1]==2
//
// Naga: `mat2` / `mat2x2` as a `struct` field is rejected (std140; see
// https://github.com/gfx-rs/wgpu/issues/4375). Matrix2D and Affine2D store the
// same data as two vec2 column vectors: `col0` ≡ m[0], `col1` ≡ m[1].
//
// Lowering: subscripted store into a matrix that is a struct field
// (e.g. `t.rotation[i][j] =`) is rejected; use a temporary `mat3` then assign
// `t.rotation = ...` (see `test_struct_mat_write_transform3d_rot_element`).

// ============================================================================
// Structs with matrix members (mat2 via vec2 column pairs, mat3, mat4)
// ============================================================================

struct Matrix2D { vec2 col0; vec2 col1; };
struct Transform3D { mat3 rotation; vec3 translation; };
struct Matrix4x4 { mat4 matrix; };
struct Affine2D { vec2 linear0; vec2 linear1; vec2 offset; };

// ----------------------------------------------------------------------------
// 1) Declaration
// ----------------------------------------------------------------------------

float test_struct_mat_declare_matrix2d() {
    Matrix2D s;
    return 1.0;
}

// @unimplemented(jit.q32)
// run: test_struct_mat_declare_matrix2d() == 1.0

int test_struct_mat_declare_matrix4x4() {
    Matrix4x4 s;
    return 1;
}

// @unimplemented(jit.q32)
// run: test_struct_mat_declare_matrix4x4() == 1

// ----------------------------------------------------------------------------
// 2) Construction + member read
// ----------------------------------------------------------------------------

float test_struct_mat_construct_m2_diagonal() {
    Matrix2D s = Matrix2D(vec2(1.0, 0.0), vec2(0.0, 1.0));
    return s.col0[0];
}

// @unimplemented(jit.q32)
// run: test_struct_mat_construct_m2_diagonal() ~= 1.0

float test_struct_mat_construct_m2_offdiag() {
    // same columns as mat2(1,2,3,4)
    Matrix2D s = Matrix2D(vec2(1.0, 2.0), vec2(3.0, 4.0));
    return s.col1[0];
}

// @unimplemented(jit.q32)
// run: test_struct_mat_construct_m2_offdiag() ~= 3.0

float test_struct_mat_construct_affine2d_offset_y() {
    Affine2D a = Affine2D(vec2(1.0, 0.0), vec2(0.0, 1.0), vec2(5.0, 7.0));
    return a.offset.y;
}

// @unimplemented(jit.q32)
// run: test_struct_mat_construct_affine2d_offset_y() ~= 7.0

vec3 test_struct_mat_construct_transform3d_translation() {
    Transform3D t = Transform3D(mat3(1.0), vec3(0.25, 0.5, 0.75));
    return t.translation;
}

// @unimplemented(jit.q32)
// run: test_struct_mat_construct_transform3d_translation() ~= vec3(0.25, 0.5, 0.75)

float test_struct_mat_construct_m4_identity_corner() {
    Matrix4x4 s = Matrix4x4(mat4(1.0));
    // Column 2 of I is (0,0,1,0) -> [2].z == 1
    return s.matrix[2].z;
}

// @unimplemented(jit.q32)
// run: test_struct_mat_construct_m4_identity_corner() ~= 1.0

// ----------------------------------------------------------------------------
// 3) Member read (column / subscripts)
// ----------------------------------------------------------------------------

vec2 test_struct_mat_read_m2_column1() {
    Matrix2D s = Matrix2D(vec2(10.0, 20.0), vec2(30.0, 40.0));
    return s.col1;
}

// @unimplemented(jit.q32)
// run: test_struct_mat_read_m2_column1() ~= vec2(30.0, 40.0)

float test_struct_mat_read_m3_column0_x() {
    Transform3D t = Transform3D(
        mat3(2.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 4.0),
        vec3(0.0)
    );
    return t.rotation[0].x;
}

// @unimplemented(jit.q32)
// run: test_struct_mat_read_m3_column0_x() ~= 2.0

vec4 test_struct_mat_read_m4_column3() {
    Matrix4x4 s = Matrix4x4(
        mat4(
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            2.0, 3.0, 4.0, 1.0
        )
    );
    return s.matrix[3];
}

// @unimplemented(jit.q32)
// run: test_struct_mat_read_m4_column3() ~= vec4(2.0, 3.0, 4.0, 1.0)

// ----------------------------------------------------------------------------
// 4) Member write
// ----------------------------------------------------------------------------

float test_struct_mat_write_m2_reassign() {
    Matrix2D s = Matrix2D(vec2(0.0, 0.0), vec2(0.0, 0.0));
    s.col0 = vec2(1.0, 0.0);
    s.col1 = vec2(0.0, 9.0);
    return s.col1[1];
}

// @unimplemented(jit.q32)
// run: test_struct_mat_write_m2_reassign() ~= 9.0

float test_struct_mat_write_affine2d_offset() {
    Affine2D a = Affine2D(vec2(1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, 0.0));
    a.offset = vec2(-1.0, 2.0);
    return a.offset.x + a.linear0[0];
}

// @unimplemented(jit.q32)
// run: test_struct_mat_write_affine2d_offset() ~= 0.0

float test_struct_mat_write_transform3d_rot_element() {
    Transform3D t = Transform3D(mat3(1.0), vec3(0.0));
    // Direct `t.rotation[i][j] =` is not lowered (struct field base); copy via local mat3.
    mat3 r = t.rotation;
    r[1][1] = 8.0;
    t.rotation = r;
    return t.rotation[1].y;
}

// @unimplemented(jit.q32)
// run: test_struct_mat_write_transform3d_rot_element() ~= 8.0

// ----------------------------------------------------------------------------
// 5) Whole-struct assignment
// ----------------------------------------------------------------------------

float test_struct_mat_assign_copy_matrix2d() {
    Matrix2D a = Matrix2D(vec2(2.0, 0.0), vec2(0.0, 3.0));
    Matrix2D b = Matrix2D(vec2(0.0), vec2(0.0));
    b = a;
    return b.col0[0] + b.col1[1];
}

// @unimplemented(jit.q32)
// run: test_struct_mat_assign_copy_matrix2d() ~= 5.0

float test_struct_mat_assign_copy_affine2d() {
    Affine2D a = Affine2D(vec2(4.0, 0.0), vec2(0.0, 4.0), vec2(1.0, 1.0));
    Affine2D b = Affine2D(vec2(1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, 0.0));
    b = a;
    return b.offset.x * b.linear0[0];
}

// @unimplemented(jit.q32)
// run: test_struct_mat_assign_copy_affine2d() ~= 4.0

vec3 test_struct_mat_assign_copy_transform3d() {
    Transform3D a = Transform3D(mat3(1.0), vec3(9.0, 8.0, 7.0));
    Transform3D b = Transform3D(mat3(0.0), vec3(0.0, 0.0, 0.0));
    b = a;
    return b.translation + b.rotation[0].xyz;
}

// @unimplemented(jit.q32)
// run: test_struct_mat_assign_copy_transform3d() ~= vec3(10.0, 8.0, 7.0)

// ----------------------------------------------------------------------------
// 6) Matrix element access
// ----------------------------------------------------------------------------

float test_struct_mat_element_m2_01() {
    Matrix2D s = Matrix2D(vec2(1.0, 0.0), vec2(0.0, 1.0));
    return s.col0[1] + s.col1[0] + s.col1[1];
}

// @unimplemented(jit.q32)
// run: test_struct_mat_element_m2_01() ~= 1.0

float test_struct_mat_element_m3_22() {
    Transform3D t = Transform3D(mat3(1.0), vec3(0.0));
    return t.rotation[2].z;
}

// @unimplemented(jit.q32)
// run: test_struct_mat_element_m3_22() ~= 1.0

float test_struct_mat_element_m4_11() {
    Matrix4x4 s = Matrix4x4(mat4(1.0));
    return s.matrix[1][1];
}

// @unimplemented(jit.q32)
// run: test_struct_mat_element_m4_11() ~= 1.0

float test_struct_mat_element_m4_nontrivial() {
    Matrix4x4 s = Matrix4x4(
        mat4(
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            1.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 1.0
        )
    );
    return s.matrix[1][2];
}

// @unimplemented(jit.q32)
// run: test_struct_mat_element_m4_nontrivial() ~= 1.0
