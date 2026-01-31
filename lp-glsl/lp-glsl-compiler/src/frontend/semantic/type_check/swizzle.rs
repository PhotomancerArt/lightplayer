//! Swizzle parsing and validation for vector component access

use crate::error::{ErrorCode, GlslError};

use alloc::format;

/// Parse swizzle string and return the number of components
/// Validates that the swizzle is valid for the given vector size
pub fn parse_swizzle_length(swizzle: &str, max_components: usize) -> Result<usize, GlslError> {
    if swizzle.is_empty() {
        return Err(GlslError::new(ErrorCode::E0113, "empty swizzle"));
    }

    if swizzle.len() > 4 {
        return Err(GlslError::new(
            ErrorCode::E0113,
            format!(
                "swizzle can have at most 4 components, got {}",
                swizzle.len()
            ),
        ));
    }

    // Determine naming set and validate consistency
    let mut xyzw_count = 0;
    let mut rgba_count = 0;
    let mut stpq_count = 0;

    for ch in swizzle.chars() {
        match ch {
            'x' | 'y' | 'z' | 'w' => xyzw_count += 1,
            'r' | 'g' | 'b' | 'a' => rgba_count += 1,
            's' | 't' | 'p' | 'q' => stpq_count += 1,
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0113,
                    format!("invalid swizzle character: '{ch}'"),
                ));
            }
        }
    }

    let sets_used = (xyzw_count > 0) as u32 + (rgba_count > 0) as u32 + (stpq_count > 0) as u32;
    if sets_used > 1 {
        return Err(GlslError::new(
            ErrorCode::E0113,
            format!("swizzle '{swizzle}' mixes component naming sets (xyzw/rgba/stpq)"),
        ));
    }

    // Validate each component is within bounds
    let _naming_set = if xyzw_count > 0 {
        ('x', 'y', 'z', 'w')
    } else if rgba_count > 0 {
        ('r', 'g', 'b', 'a')
    } else {
        ('s', 't', 'p', 'q')
    };

    for ch in swizzle.chars() {
        let idx = match ch {
            'x' | 'r' | 's' => 0,
            'y' | 'g' | 't' => 1,
            'z' | 'b' | 'p' => 2,
            'w' | 'a' | 'q' => 3,
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0113,
                    format!("invalid component '{ch}'"),
                ));
            }
        };

        if idx >= max_components {
            return Err(GlslError::new(
                ErrorCode::E0111,
                format!("component '{ch}' not valid for vector with {max_components} components"),
            ));
        }
    }

    Ok(swizzle.len())
}
