/// Macro to generate serialization boilerplate for StateField-wrapped state structs
///
/// Usage:
/// ```ignore
/// impl_state_serialization! {
///     FixtureState => SerializableFixtureState {
///         lamp_colors: Vec<u8>,
///         mapping_cells: Vec<MappingCell>,
///         texture_handle: Option<NodeHandle>,
///         output_handle: Option<NodeHandle>,
///     }
/// }
///
/// impl_state_serialization! {
///     TextureState => SerializableTextureState {
///         #[base64] texture_data: Vec<u8>,
///         width: u32,
///         height: u32,
///         format: String,
///     }
/// }
/// ```
#[macro_export]
macro_rules! impl_state_serialization {
    (
        $state_name:ident => $wrapper_name:ident {
            $($fields:tt)*
        }
    ) => {
        impl_state_serialization!(@parse_fields
            $state_name,
            $wrapper_name,
            (),
            $($fields)*
        );
    };

    // Parse fields one by one - base64 field
    (@parse_fields
        $state_name:ident,
        $wrapper_name:ident,
        ($($parsed:tt)*),
        #[base64] $field:ident: Vec<u8> $(, $($rest:tt)*)?
    ) => {
        impl_state_serialization!(@parse_fields
            $state_name,
            $wrapper_name,
            ($($parsed)* (#[base64] $field: Vec<u8>)),
            $($($rest)*)?
        );
    };
    // Parse fields one by one - normal field
    (@parse_fields
        $state_name:ident,
        $wrapper_name:ident,
        ($($parsed:tt)*),
        $field:ident: $field_type:ty $(, $($rest:tt)*)?
    ) => {
        impl_state_serialization!(@parse_fields
            $state_name,
            $wrapper_name,
            ($($parsed)* ($field: $field_type)),
            $($($rest)*)?
        );
    };
    // End of parsing - generate code
    (@parse_fields
        $state_name:ident,
        $wrapper_name:ident,
        ($($parsed:tt)*),
    ) => {
        impl_state_serialization!(@generate
            $state_name,
            $wrapper_name,
            $($parsed)*
        );
    };

    // Generate all the boilerplate
    (@generate
        $state_name:ident,
        $wrapper_name:ident,
        $($field_spec:tt)*
    ) => {
        /// Wrapper for serializing $state_name with a since_frame context
        pub struct $wrapper_name<'a> {
            state: &'a $state_name,
            since_frame: FrameId,
        }

        impl<'a> $wrapper_name<'a> {
            pub fn new(state: &'a $state_name, since_frame: FrameId) -> Self {
                Self { state, since_frame }
            }
        }

        impl<'a> Serialize for $wrapper_name<'a> {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let is_initial_sync = self.since_frame == FrameId::default();
                let field_count = impl_state_serialization!(@count $($field_spec)*);
                let mut state = serializer.serialize_struct(stringify!($state_name), field_count)?;

                $(
                    impl_state_serialization!(@serialize_wrapper_field
                        self,
                        state,
                        is_initial_sync,
                        $field_spec
                    );
                )*

                state.end()
            }
        }

        // Temporary: Simple Serialize implementation for NodeState compatibility
        impl Serialize for $state_name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let field_count = impl_state_serialization!(@count $($field_spec)*);
                let mut state = serializer.serialize_struct(stringify!($state_name), field_count)?;

                $(
                    impl_state_serialization!(@serialize_direct_field
                        self,
                        state,
                        $field_spec
                    );
                )*

                state.end()
            }
        }

        impl<'de> Deserialize<'de> for $state_name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                impl_state_serialization!(@gen_helper_fields $state_name, (), $($field_spec)*);

                impl_state_serialization!(@deserialize_helper $state_name, deserializer, helper);

                let frame_id = FrameId::default();
                let mut state = $state_name::new(frame_id);

                $(
                    impl_state_serialization!(@deserialize_field
                        helper,
                        state,
                        frame_id,
                        $field_spec
                    );
                )*

                Ok(state)
            }
        }
    };

    // Count fields
    (@count) => { 0 };
    (@count $spec:tt $($rest:tt)*) => {
        1 + impl_state_serialization!(@count $($rest)*)
    };

    // Serialize field in wrapper - base64 case
    (@serialize_wrapper_field $self:expr, $state:ident, $is_initial_sync:expr, (#[base64] $field:ident: Vec<u8>)) => {
        if $is_initial_sync || $self.state.$field.changed_frame() > $self.since_frame {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode($self.state.$field.value());
            $state.serialize_field(stringify!($field), &encoded)?;
        }
    };
    // Serialize field in wrapper - Option<String> where None means "cleared"
    // Use "" as sentinel so client can distinguish "cleared" from "omitted"
    (@serialize_wrapper_field $self:expr, $state:ident, $is_initial_sync:expr, ($field:ident: Option<String>)) => {
        if $is_initial_sync || $self.state.$field.changed_frame() > $self.since_frame {
            let s: &str = $self.state.$field.value().as_deref().unwrap_or("");
            $state.serialize_field(stringify!($field), s)?;
        }
    };
    // Serialize field in wrapper - normal case
    (@serialize_wrapper_field $self:expr, $state:ident, $is_initial_sync:expr, ($field:ident: $field_type:ty)) => {
        if $is_initial_sync || $self.state.$field.changed_frame() > $self.since_frame {
            $state.serialize_field(stringify!($field), $self.state.$field.value())?;
        }
    };

    // Serialize field directly - base64 case
    (@serialize_direct_field $self:expr, $state:ident, (#[base64] $field:ident: Vec<u8>)) => {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode($self.$field.value());
        $state.serialize_field(stringify!($field), &encoded)?;
    };
    // Serialize field directly - normal case
    (@serialize_direct_field $self:expr, $state:ident, ($field:ident: $field_type:ty)) => {
        $state.serialize_field(stringify!($field), $self.$field.value())?;
    };

    // Generate helper struct - FixtureState
    (@gen_helper FixtureState, $(#[base64] $base64_field:ident: Vec<u8>)* $($normal_field:ident: $normal_field_type:ty)*) => {
        #[derive(Deserialize)]
        struct FixtureStateHelper {
            $(
                $base64_field: Option<String>,
            )*
            $(
                $normal_field: Option<$normal_field_type>,
            )*
        }
    };
    // Generate helper struct - TextureState
    (@gen_helper TextureState, $(#[base64] $base64_field:ident: Vec<u8>)* $($normal_field:ident: $normal_field_type:ty)*) => {
        #[derive(Deserialize)]
        struct TextureStateHelper {
            $(
                $base64_field: Option<String>,
            )*
            $(
                $normal_field: Option<$normal_field_type>,
            )*
        }
    };
    // Generate helper struct - OutputState
    (@gen_helper OutputState, $(#[base64] $base64_field:ident: Vec<u8>)* $($normal_field:ident: $normal_field_type:ty)*) => {
        #[derive(Deserialize)]
        struct OutputStateHelper {
            $(
                $base64_field: Option<String>,
            )*
            $(
                $normal_field: Option<$normal_field_type>,
            )*
        }
    };
    // Generate helper struct - ShaderState
    (@gen_helper ShaderState, $(#[base64] $base64_field:ident: Vec<u8>)* $($normal_field:ident: $normal_field_type:ty)*) => {
        #[derive(Deserialize)]
        struct ShaderStateHelper {
            $(
                $base64_field: Option<String>,
            )*
            $(
                $normal_field: Option<$normal_field_type>,
            )*
        }
    };

    // Process fields and generate helper struct
    (@gen_helper_fields FixtureState, ($($fields:tt)*), (#[base64] $field:ident: Vec<u8>) $($rest:tt)*) => {
        impl_state_serialization!(@gen_helper_fields FixtureState, ($($fields)* $field: Option<String>,), $($rest)*);
    };
    (@gen_helper_fields FixtureState, ($($fields:tt)*), ($field:ident: $field_type:ty) $($rest:tt)*) => {
        impl_state_serialization!(@gen_helper_fields FixtureState, ($($fields)* $field: Option<$field_type>,), $($rest)*);
    };
    (@gen_helper_fields FixtureState, ($($fields:tt)*),) => {
        #[derive(Deserialize)]
        struct FixtureStateHelper {
            $($fields)*
        }
    };
    (@gen_helper_fields TestState, ($($fields:tt)*), (#[base64] $field:ident: Vec<u8>) $($rest:tt)*) => {
        impl_state_serialization!(@gen_helper_fields TestState, ($($fields)* $field: Option<String>,), $($rest)*);
    };
    (@gen_helper_fields TestState, ($($fields:tt)*), ($field:ident: $field_type:ty) $($rest:tt)*) => {
        impl_state_serialization!(@gen_helper_fields TestState, ($($fields)* $field: Option<$field_type>,), $($rest)*);
    };
    (@gen_helper_fields TestState, ($($fields:tt)*),) => {
        #[derive(Deserialize)]
        struct TestStateHelper {
            $($fields)*
        }
    };
    (@gen_helper_fields TextureState, ($($fields:tt)*), (#[base64] $field:ident: Vec<u8>) $($rest:tt)*) => {
        impl_state_serialization!(@gen_helper_fields TextureState, ($($fields)* $field: Option<String>,), $($rest)*);
    };
    (@gen_helper_fields TextureState, ($($fields:tt)*), ($field:ident: $field_type:ty) $($rest:tt)*) => {
        impl_state_serialization!(@gen_helper_fields TextureState, ($($fields)* $field: Option<$field_type>,), $($rest)*);
    };
    (@gen_helper_fields TextureState, ($($fields:tt)*),) => {
        #[derive(Deserialize)]
        struct TextureStateHelper {
            $($fields)*
        }
    };
    (@gen_helper_fields OutputState, ($($fields:tt)*), (#[base64] $field:ident: Vec<u8>) $($rest:tt)*) => {
        impl_state_serialization!(@gen_helper_fields OutputState, ($($fields)* $field: Option<String>,), $($rest)*);
    };
    (@gen_helper_fields OutputState, ($($fields:tt)*), ($field:ident: $field_type:ty) $($rest:tt)*) => {
        impl_state_serialization!(@gen_helper_fields OutputState, ($($fields)* $field: Option<$field_type>,), $($rest)*);
    };
    (@gen_helper_fields OutputState, ($($fields:tt)*),) => {
        #[derive(Deserialize)]
        struct OutputStateHelper {
            $($fields)*
        }
    };
    (@gen_helper_fields ShaderState, ($($fields:tt)*), (#[base64] $field:ident: Vec<u8>) $($rest:tt)*) => {
        impl_state_serialization!(@gen_helper_fields ShaderState, ($($fields)* $field: Option<String>,), $($rest)*);
    };
    (@gen_helper_fields ShaderState, ($($fields:tt)*), ($field:ident: $field_type:ty) $($rest:tt)*) => {
        impl_state_serialization!(@gen_helper_fields ShaderState, ($($fields)* $field: Option<$field_type>,), $($rest)*);
    };
    (@gen_helper_fields ShaderState, ($($fields:tt)*),) => {
        #[derive(Deserialize)]
        struct ShaderStateHelper {
            $($fields)*
        }
    };

    // Deserialize field - base64 case
    (@deserialize_field $helper:expr, $state:ident, $frame_id:expr, (#[base64] $field:ident: Vec<u8>)) => {
        if let Some(encoded) = $helper.$field {
            use base64::Engine;
            match base64::engine::general_purpose::STANDARD.decode(&encoded) {
                Ok(decoded) => {
                    $state.$field.set($frame_id, decoded);
                }
                Err(_) => {
                    // Invalid base64, leave as default
                }
            }
        }
    };
    // Deserialize field - normal case
    (@deserialize_field $helper:expr, $state:ident, $frame_id:expr, ($field:ident: $field_type:ty)) => {
        if let Some(val) = $helper.$field {
            $state.$field.set($frame_id, val);
        }
    };

    // Helper struct name
    (@helper_name FixtureState) => { FixtureStateHelper };
    (@helper_name TextureState) => { TextureStateHelper };
    (@helper_name OutputState) => { OutputStateHelper };
    (@helper_name ShaderState) => { ShaderStateHelper };
    (@helper_name TestState) => { TestStateHelper };

    // Deserialize helper
    (@deserialize_helper FixtureState, $deserializer:expr, $helper:ident) => {
        let $helper = <FixtureStateHelper as serde::Deserialize>::deserialize($deserializer)?;
    };
    (@deserialize_helper TextureState, $deserializer:expr, $helper:ident) => {
        let $helper = <TextureStateHelper as serde::Deserialize>::deserialize($deserializer)?;
    };
    (@deserialize_helper OutputState, $deserializer:expr, $helper:ident) => {
        let $helper = <OutputStateHelper as serde::Deserialize>::deserialize($deserializer)?;
    };
    (@deserialize_helper ShaderState, $deserializer:expr, $helper:ident) => {
        let $helper = <ShaderStateHelper as serde::Deserialize>::deserialize($deserializer)?;
    };
    (@deserialize_helper TestState, $deserializer:expr, $helper:ident) => {
        let $helper = <TestStateHelper as serde::Deserialize>::deserialize($deserializer)?;
    };
}
