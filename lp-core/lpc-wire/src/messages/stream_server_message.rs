//! Direct JSON writers for server message envelopes.

use crate::json::json_write::JsonWrite;
use crate::json::json_writer::{JsonWriter, JsonWriterError};
use crate::message::envelope::ServerMessage;
use crate::messages::{ProjectReadResponse, write_project_read_response};
use crate::server::ServerMsgBody;

/// Write a project-read server message without buffering the whole JSON frame.
pub fn write_project_read_server_message<W>(
    mut writer: JsonWriter<W>,
    id: u64,
    response: &ProjectReadResponse,
) -> Result<W, JsonWriterError<W::Error>>
where
    W: JsonWrite,
{
    writer.write_raw(b"{\"id\":")?;
    writer.u64(id)?;
    writer.write_raw(b",\"msg\":{\"projectRequest\":{\"response\":")?;
    let out = write_project_read_response(writer, response)?;
    let mut writer = JsonWriter::new(out);
    writer.write_raw(b"}}}")?;
    Ok(writer.into_inner())
}

/// Write any server message, using direct writers for large known variants.
pub fn write_server_message<W>(
    writer: JsonWriter<W>,
    message: &ServerMessage<ProjectReadResponse>,
) -> Result<W, JsonWriterError<W::Error>>
where
    W: JsonWrite,
{
    match &message.msg {
        ServerMsgBody::ProjectRequest { response } => {
            write_project_read_server_message(writer, message.id, response)
        }
        _ => {
            let mut writer = writer;
            writer.serde(message)?;
            Ok(writer.into_inner())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;
    use crate::messages::{ProjectReadResult, ReadLevel, ShapeReadResult};
    use alloc::vec;
    use alloc::vec::Vec;
    use lpc_model::Revision;

    #[test]
    fn project_read_server_message_direct_writer_matches_serde_shape() {
        let response = ProjectReadResponse {
            revision: Revision::new(12),
            results: vec![ProjectReadResult::Shapes(ShapeReadResult {
                level: ReadLevel::Ids,
                registry: None,
            })],
            probes: vec![],
            mutations: vec![],
        };
        let bytes = write_project_read_server_message(JsonWriter::new(Vec::new()), 42, &response)
            .expect("write server message");
        let decoded: ServerMessage<ProjectReadResponse> =
            json::from_slice(&bytes).expect("decode direct-written message");

        assert_eq!(decoded.id, 42);
        let ServerMsgBody::ProjectRequest { response: decoded } = decoded.msg else {
            panic!("expected project request response");
        };
        assert_eq!(decoded, response);
    }
}
