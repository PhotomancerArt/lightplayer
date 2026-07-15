//! The unsolicited wire hello is the first id-0 frame a fw-host runtime
//! sends when its server loop starts serving (M2 bootstrap contract, see
//! docs/adr/2026-07-14-wire-hello-versioning.md).

use fw_host::HostRuntime;
use lpc_wire::{WIRE_PROTO_VERSION, server::ServerMsgBody};

#[tokio::test]
async fn unsolicited_hello_arrives_as_the_first_id0_frame() {
    let mut runtime = HostRuntime::start_memory().unwrap();

    // Read the raw transport (not TokioLpClient, which folds unsolicited
    // frames into events): the very first frame off the wire must be the
    // id-0 hello.
    let first = {
        let transport = runtime.client_transport();
        let mut transport = transport.lock().await;
        transport.receive().await.expect("first frame")
    };

    assert_eq!(first.id, 0, "hello must be unsolicited (id 0)");
    match first.msg {
        ServerMsgBody::Hello(hello) => {
            assert_eq!(hello.proto, WIRE_PROTO_VERSION);
            assert_eq!(hello.fw.package, "fw-host");
            assert_eq!(hello.device_uid, None);
        }
        other => panic!("expected the hello as the first frame, got {other:?}"),
    }

    runtime.close().await.unwrap();
}
