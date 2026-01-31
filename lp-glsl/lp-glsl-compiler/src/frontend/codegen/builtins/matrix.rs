//! Matrix built-in functions

use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::types::Type;
use cranelift_codegen::ir::{InstBuilder, Value};

use alloc::{format, vec, vec::Vec};
#[allow(
    non_snake_case,
    reason = "Matrix function names follow GLSL naming convention"
)]
impl<'a, M: cranelift_module::Module> CodegenContext<'a, M> {
    /// Component-wise matrix multiply: result[i][j] = x[i][j] * y[i][j]
    pub fn builtin_matrixCompMult(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];
        let (y_vals, y_ty) = &args[1];

        if x_ty != y_ty || !x_ty.is_matrix() {
            return Err(GlslError::new(
                ErrorCode::E0104,
                "matrixCompMult() requires two matrices of the same type",
            ));
        }

        let mut result_vals = Vec::new();
        for (x_val, y_val) in x_vals.iter().zip(y_vals.iter()) {
            result_vals.push(self.builder.ins().fmul(*x_val, *y_val));
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// Outer product: vec1 × vec2 → matrix
    /// For vec3 × vec3, returns mat3 where result[col][row] = vec1[col] * vec2[row]
    pub fn builtin_outerProduct(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (vec1_vals, vec1_ty) = &args[0];
        let (vec2_vals, vec2_ty) = &args[1];

        if !vec1_ty.is_vector() || !vec2_ty.is_vector() {
            return Err(GlslError::new(
                ErrorCode::E0104,
                "outerProduct() requires two vectors",
            ));
        }

        let vec1_size = vec1_vals.len();
        let vec2_size = vec2_vals.len();

        // Determine result matrix type based on vector sizes
        let result_ty = match (vec1_size, vec2_size) {
            (2, 2) => Type::Mat2,
            (3, 3) => Type::Mat3,
            (4, 4) => Type::Mat4,
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0104,
                    format!(
                        "outerProduct() requires matching vector sizes (got {vec1_size} and {vec2_size})"
                    ),
                ));
            }
        };

        // Compute outer product: result[col][row] = vec1[col] * vec2[row]
        // Result matrix is stored column-major
        let mut result_vals = Vec::new();
        for col in 0..vec1_size {
            // Columns come from vec1
            for row in 0..vec2_size {
                // Rows come from vec2
                // result[col][row] = c[col] * r[row]
                let product = self.builder.ins().fmul(vec1_vals[col], vec2_vals[row]);
                result_vals.push(product);
            }
        }

        Ok((result_vals, result_ty))
    }

    /// Transpose matrix: swap rows and columns
    pub fn builtin_transpose(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (m_vals, m_ty) = &args[0];

        if !m_ty.is_matrix() {
            return Err(GlslError::new(
                ErrorCode::E0104,
                "transpose() requires a matrix",
            ));
        }

        let (rows, cols) = m_ty.matrix_dims().unwrap();

        // Transpose: result[col][row] = m[row][col]
        // Input is column-major: m_vals[col * rows + row] = m[col][row]
        // Output should be column-major: result_vals[col * rows + row] = result[col][row] = m[row][col]
        // m[row][col] = m_vals[row * rows + col] (since row becomes column index)
        // So: result_vals[result_col * rows + result_row] = m_vals[result_row * rows + result_col]
        let mut result_vals = Vec::new();
        for result_col in 0..rows {
            // Transposed matrix has rows columns
            for result_row in 0..cols {
                // Transposed matrix has cols rows
                // result[result_col][result_row] = m[result_row][result_col]
                // m[result_row][result_col] is at: result_row * rows + result_col
                let old_idx = result_row * rows + result_col;
                result_vals.push(m_vals[old_idx]);
            }
        }

        // Result type is the same (square matrices)
        Ok((result_vals, m_ty.clone()))
    }

    /// Helper to compute 2x2 determinant from column-major values
    /// Values should be 4 elements: [col0_row0, col0_row1, col1_row0, col1_row1]
    fn compute_2x2_determinant(&mut self, vals: &[Value]) -> Value {
        // Helper to get element at (row, col) from column-major storage
        let get = |row: usize, col: usize| -> Value { vals[col * 2 + row] };

        // det = a*d - b*c
        // Matrix: [a b]  stored as [a, c, b, d] (col-major)
        //         [c d]
        let a = get(0, 0);
        let b = get(0, 1);
        let c = get(1, 0);
        let d = get(1, 1);

        let ad = self.builder.ins().fmul(a, d);
        let bc = self.builder.ins().fmul(b, c);
        self.builder.ins().fsub(ad, bc)
    }

    /// Helper to compute 3x3 determinant from column-major values
    /// Values should be 9 elements: [col0_row0, col0_row1, col0_row2, col1_row0, ...]
    fn compute_3x3_determinant(&mut self, vals: &[Value]) -> Value {
        // Helper to get element at (row, col) from column-major storage
        let get = |row: usize, col: usize| -> Value { vals[col * 3 + row] };

        // Sarrus rule for 3x3
        // det = a(ei - fh) - b(di - fg) + c(dh - eg)
        let a = get(0, 0);
        let b = get(0, 1);
        let c = get(0, 2);
        let d = get(1, 0);
        let e = get(1, 1);
        let f = get(1, 2);
        let g = get(2, 0);
        let h = get(2, 1);
        let i = get(2, 2);

        let ei = self.builder.ins().fmul(e, i);
        let fh = self.builder.ins().fmul(f, h);
        let ei_minus_fh = self.builder.ins().fsub(ei, fh);
        let term1 = self.builder.ins().fmul(a, ei_minus_fh);

        let di = self.builder.ins().fmul(d, i);
        let fg = self.builder.ins().fmul(f, g);
        let di_minus_fg = self.builder.ins().fsub(di, fg);
        let term2 = self.builder.ins().fmul(b, di_minus_fg);

        let dh = self.builder.ins().fmul(d, h);
        let eg = self.builder.ins().fmul(e, g);
        let dh_minus_eg = self.builder.ins().fsub(dh, eg);
        let term3 = self.builder.ins().fmul(c, dh_minus_eg);

        let term1_minus_term2 = self.builder.ins().fsub(term1, term2);
        self.builder.ins().fadd(term1_minus_term2, term3)
    }

    /// Compute matrix determinant
    pub fn builtin_determinant(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (m_vals, m_ty) = &args[0];

        if !m_ty.is_matrix() {
            return Err(GlslError::new(
                ErrorCode::E0104,
                "determinant() requires a matrix",
            ));
        }

        let (rows, cols) = m_ty.matrix_dims().unwrap();
        if rows != cols {
            return Err(GlslError::new(
                ErrorCode::E0104,
                "determinant() only supported for square matrices",
            ));
        }

        // Helper to get element at (row, col) from column-major storage
        let get = |row: usize, col: usize| -> Value { m_vals[col * rows + row] };

        let det = match rows {
            2 => {
                // det = a*d - b*c
                // Matrix: [a b]  stored as [a, c, b, d] (col-major)
                //         [c d]
                let a = get(0, 0);
                let b = get(0, 1);
                let c = get(1, 0);
                let d = get(1, 1);
                let ad = self.builder.ins().fmul(a, d);
                let bc = self.builder.ins().fmul(b, c);
                self.builder.ins().fsub(ad, bc)
            }
            3 => {
                // Sarrus rule for 3x3
                // det = a(ei - fh) - b(di - fg) + c(dh - eg)
                let a = get(0, 0);
                let b = get(0, 1);
                let c = get(0, 2);
                let d = get(1, 0);
                let e = get(1, 1);
                let f = get(1, 2);
                let g = get(2, 0);
                let h = get(2, 1);
                let i = get(2, 2);

                let ei = self.builder.ins().fmul(e, i);
                let fh = self.builder.ins().fmul(f, h);
                let ei_minus_fh = self.builder.ins().fsub(ei, fh);
                let term1 = self.builder.ins().fmul(a, ei_minus_fh);

                let di = self.builder.ins().fmul(d, i);
                let fg = self.builder.ins().fmul(f, g);
                let di_minus_fg = self.builder.ins().fsub(di, fg);
                let term2 = self.builder.ins().fmul(b, di_minus_fg);

                let dh = self.builder.ins().fmul(d, h);
                let eg = self.builder.ins().fmul(e, g);
                let dh_minus_eg = self.builder.ins().fsub(dh, eg);
                let term3 = self.builder.ins().fmul(c, dh_minus_eg);

                let term1_minus_term2 = self.builder.ins().fsub(term1, term2);
                self.builder.ins().fadd(term1_minus_term2, term3)
            }
            4 => {
                // Cofactor expansion for 4x4 (using first row, 0-indexed)
                // det(M) = Σ(-1)^(0+j) * M[0][j] * det(M_minor_j) = Σ(-1)^j * M[0][j] * det(M_minor_j)
                // Where M_minor_j is the 3x3 matrix obtained by removing row 0 and column j

                let zero = self.builder.ins().f32const(0.0);
                let mut det = zero;

                // Expand along first row (row 0)
                for j in 0..4 {
                    // Get element M[0][j] from first row
                    let m_0j = get(0, j);

                    // Compute sign: (-1)^j = 1 if j is even, -1 if j is odd
                    let sign = if j % 2 == 0 {
                        self.builder.ins().f32const(1.0)
                    } else {
                        self.builder.ins().f32const(-1.0)
                    };

                    // Extract 3x3 minor matrix by removing row 0 and column j
                    // Minor matrix uses rows 1,2,3 and columns 0,1,2,3 except column j
                    let mut minor_vals = Vec::new();
                    // Iterate over the 3 columns of the minor matrix
                    for minor_col in 0..3 {
                        // Map minor column index to original column index
                        // Original columns are [0,1,2,3] with j removed
                        // If minor_col < j: original column = minor_col
                        // If minor_col >= j: original column = minor_col + 1 (skip j)
                        let orig_col = if minor_col < j {
                            minor_col
                        } else {
                            minor_col + 1
                        };
                        // Extract rows 1, 2, 3 (skip row 0)
                        for minor_row in 0..3 {
                            let orig_row = minor_row + 1; // Skip row 0
                            minor_vals.push(get(orig_row, orig_col));
                        }
                    }

                    // Compute determinant of minor
                    let minor_det = self.compute_3x3_determinant(&minor_vals);

                    // Compute: sign * M[0][j] * det(minor)
                    let m_times_det = self.builder.ins().fmul(m_0j, minor_det);
                    let cofactor = self.builder.ins().fmul(sign, m_times_det);

                    // Accumulate: det += cofactor
                    det = self.builder.ins().fadd(det, cofactor);
                }

                det
            }
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0104,
                    format!("determinant() not supported for {rows}-dimensional matrices"),
                ));
            }
        };

        Ok((vec![det], Type::Float))
    }

    /// Compute matrix inverse
    pub fn builtin_inverse(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (m_vals, m_ty) = &args[0];

        if !m_ty.is_matrix() {
            return Err(GlslError::new(
                ErrorCode::E0104,
                "inverse() requires a matrix",
            ));
        }

        let (rows, cols) = m_ty.matrix_dims().unwrap();
        if rows != cols {
            return Err(GlslError::new(
                ErrorCode::E0104,
                "inverse() only supported for square matrices",
            ));
        }

        // Helper to get element at (row, col) from column-major storage
        let get = |row: usize, col: usize| -> Value { m_vals[col * rows + row] };

        match rows {
            2 => {
                // Inverse of 2x2: (1/det) * [d -b]
                //                              [-c a]
                // Matrix: [a b]  stored as [a, c, b, d] (col-major)
                //         [c d]
                let a = get(0, 0);
                let b = get(0, 1);
                let c = get(1, 0);
                let d = get(1, 1);

                // Compute determinant
                let ad = self.builder.ins().fmul(a, d);
                let bc = self.builder.ins().fmul(b, c);
                let det = self.builder.ins().fsub(ad, bc);

                // Compute 1/det
                let one = self.builder.ins().f32const(1.0);
                let inv_det = self.builder.ins().fdiv(one, det);

                // Compute inverse elements (stored column-major)
                // result[0][0] = d * inv_det
                // result[1][0] = -c * inv_det
                // result[0][1] = -b * inv_det
                // result[1][1] = a * inv_det
                let zero = self.builder.ins().f32const(0.0);
                let minus_c = self.builder.ins().fsub(zero, c);
                let minus_b = self.builder.ins().fsub(zero, b);

                let mut result_vals = Vec::new();
                result_vals.push(self.builder.ins().fmul(d, inv_det)); // result[0][0]
                result_vals.push(self.builder.ins().fmul(minus_c, inv_det)); // result[1][0]
                result_vals.push(self.builder.ins().fmul(minus_b, inv_det)); // result[0][1]
                result_vals.push(self.builder.ins().fmul(a, inv_det)); // result[1][1]

                Ok((result_vals, m_ty.clone()))
            }
            3 => {
                // 3x3 inverse using adjugate method: M^(-1) = (1/det(M)) * adj(M)
                // Where adj(M) is the transpose of the cofactor matrix

                // Step 1: Compute determinant
                let det = {
                    let a = get(0, 0);
                    let b = get(0, 1);
                    let c = get(0, 2);
                    let d = get(1, 0);
                    let e = get(1, 1);
                    let f = get(1, 2);
                    let g = get(2, 0);
                    let h = get(2, 1);
                    let i = get(2, 2);

                    let ei = self.builder.ins().fmul(e, i);
                    let fh = self.builder.ins().fmul(f, h);
                    let ei_minus_fh = self.builder.ins().fsub(ei, fh);
                    let term1 = self.builder.ins().fmul(a, ei_minus_fh);

                    let di = self.builder.ins().fmul(d, i);
                    let fg = self.builder.ins().fmul(f, g);
                    let di_minus_fg = self.builder.ins().fsub(di, fg);
                    let term2 = self.builder.ins().fmul(b, di_minus_fg);

                    let dh = self.builder.ins().fmul(d, h);
                    let eg = self.builder.ins().fmul(e, g);
                    let dh_minus_eg = self.builder.ins().fsub(dh, eg);
                    let term3 = self.builder.ins().fmul(c, dh_minus_eg);

                    let term1_minus_term2 = self.builder.ins().fsub(term1, term2);
                    self.builder.ins().fadd(term1_minus_term2, term3)
                };

                // Step 2: Compute 1/det
                let one = self.builder.ins().f32const(1.0);
                let inv_det = self.builder.ins().fdiv(one, det);

                // Step 3: Compute cofactor matrix
                // Cofactor[i][j] = (-1)^(i+j) * det(minor_ij)
                // Where minor_ij is the 2x2 matrix obtained by removing row i and column j
                let mut cofactor_vals = Vec::new();

                for i in 0..3 {
                    for j in 0..3 {
                        // Compute sign: (-1)^(i+j)
                        let sign = if (i + j) % 2 == 0 {
                            self.builder.ins().f32const(1.0)
                        } else {
                            self.builder.ins().f32const(-1.0)
                        };

                        // Extract 2x2 minor by removing row i and column j
                        let mut minor_vals = Vec::new();
                        for minor_col in 0..2 {
                            // Map minor column index to original column index
                            let orig_col = if minor_col < j {
                                minor_col
                            } else {
                                minor_col + 1
                            };
                            for minor_row in 0..2 {
                                // Map minor row index to original row index
                                let orig_row = if minor_row < i {
                                    minor_row
                                } else {
                                    minor_row + 1
                                };
                                minor_vals.push(get(orig_row, orig_col));
                            }
                        }

                        // Compute determinant of minor
                        let minor_det = self.compute_2x2_determinant(&minor_vals);

                        // Cofactor = sign * det(minor)
                        let cofactor = self.builder.ins().fmul(sign, minor_det);
                        cofactor_vals.push(cofactor);
                    }
                }

                // Step 4: Transpose cofactor matrix to get adjugate
                // adj[i][j] = cofactor[j][i]
                // Result stored column-major: result[col][row] = adj[row][col] = cofactor[col][row]
                let mut result_vals = Vec::new();
                for result_col in 0..3 {
                    for result_row in 0..3 {
                        // result[result_col][result_row] = adj[result_row][result_col] = cofactor[result_col][result_row]
                        let cofactor_idx = result_col * 3 + result_row;
                        let adjugate_val = cofactor_vals[cofactor_idx];

                        // Step 5: Multiply by 1/det
                        let inv_val = self.builder.ins().fmul(adjugate_val, inv_det);
                        result_vals.push(inv_val);
                    }
                }

                Ok((result_vals, m_ty.clone()))
            }
            4 => {
                // 4x4 inverse using adjugate method: M^(-1) = (1/det(M)) * adj(M)
                // Where adj(M) is the transpose of the cofactor matrix

                // Step 1: Compute determinant
                let det = {
                    let zero = self.builder.ins().f32const(0.0);
                    let mut det = zero;

                    // Expand along first row (row 0)
                    for j in 0..4 {
                        let m_0j = get(0, j);

                        let sign = if j % 2 == 0 {
                            self.builder.ins().f32const(1.0)
                        } else {
                            self.builder.ins().f32const(-1.0)
                        };

                        // Extract 3x3 minor matrix by removing row 0 and column j
                        let mut minor_vals = Vec::new();
                        for minor_col in 0..3 {
                            let orig_col = if minor_col < j {
                                minor_col
                            } else {
                                minor_col + 1
                            };
                            for minor_row in 0..3 {
                                let orig_row = minor_row + 1; // Skip row 0
                                minor_vals.push(get(orig_row, orig_col));
                            }
                        }

                        let minor_det = self.compute_3x3_determinant(&minor_vals);
                        let m_times_det = self.builder.ins().fmul(m_0j, minor_det);
                        let cofactor = self.builder.ins().fmul(sign, m_times_det);
                        det = self.builder.ins().fadd(det, cofactor);
                    }
                    det
                };

                // Step 2: Compute 1/det
                let one = self.builder.ins().f32const(1.0);
                let inv_det = self.builder.ins().fdiv(one, det);

                // Step 3: Compute cofactor matrix
                // Cofactor[i][j] = (-1)^(i+j) * det(minor_ij)
                // Where minor_ij is the 3x3 matrix obtained by removing row i and column j
                let mut cofactor_vals = Vec::new();

                for i in 0..4 {
                    for j in 0..4 {
                        // Compute sign: (-1)^(i+j)
                        let sign = if (i + j) % 2 == 0 {
                            self.builder.ins().f32const(1.0)
                        } else {
                            self.builder.ins().f32const(-1.0)
                        };

                        // Extract 3x3 minor by removing row i and column j
                        let mut minor_vals = Vec::new();
                        for minor_col in 0..3 {
                            // Map minor column index to original column index
                            let orig_col = if minor_col < j {
                                minor_col
                            } else {
                                minor_col + 1
                            };
                            for minor_row in 0..3 {
                                // Map minor row index to original row index
                                let orig_row = if minor_row < i {
                                    minor_row
                                } else {
                                    minor_row + 1
                                };
                                minor_vals.push(get(orig_row, orig_col));
                            }
                        }

                        // Compute determinant of minor
                        let minor_det = self.compute_3x3_determinant(&minor_vals);

                        // Cofactor = sign * det(minor)
                        let cofactor = self.builder.ins().fmul(sign, minor_det);
                        cofactor_vals.push(cofactor);
                    }
                }

                // Step 4: Transpose cofactor matrix to get adjugate
                // adj[i][j] = cofactor[j][i]
                // Result stored column-major: result[col][row] = adj[row][col] = cofactor[col][row]
                let mut result_vals = Vec::new();
                for result_col in 0..4 {
                    for result_row in 0..4 {
                        // result[result_col][result_row] = adj[result_row][result_col] = cofactor[result_col][result_row]
                        let cofactor_idx = result_col * 4 + result_row;
                        let adjugate_val = cofactor_vals[cofactor_idx];

                        // Step 5: Multiply by 1/det
                        let inv_val = self.builder.ins().fmul(adjugate_val, inv_det);
                        result_vals.push(inv_val);
                    }
                }

                Ok((result_vals, m_ty.clone()))
            }
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0104,
                    format!("inverse() not supported for {rows}-dimensional matrices"),
                ));
            }
        }
    }
}
