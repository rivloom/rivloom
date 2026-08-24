use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use pretty_assertions::assert_eq;

use super::ProcessControl;
use super::ProcessTransport;
use super::TransportEvent;
use super::TransportReadError;

#[test]
fn fragmented_and_batched_stdout_is_returned_as_complete_lines() {
    let (events, receiver) = mpsc::channel();
    let mut transport = ProcessTransport::new(Arc::new(NoopControl), receiver);

    events
        .send(TransportEvent::Stdout(b"{\"id\":1".to_vec()))
        .unwrap();
    events
        .send(TransportEvent::Stdout(
            b",\"result\":{}}\n{\"method\":\"ready\",\"params\":{}}\r\n".to_vec(),
        ))
        .unwrap();

    assert_eq!(
        transport
            .receive_line(Duration::from_millis(/*millis*/ 10))
            .unwrap(),
        r#"{"id":1,"result":{}}"#
    );
    assert_eq!(
        transport
            .receive_line(Duration::from_millis(/*millis*/ 10))
            .unwrap(),
        r#"{"method":"ready","params":{}}"#
    );
}

#[test]
fn timeout_and_termination_remain_distinct() {
    let (events, receiver) = mpsc::channel();
    let mut transport = ProcessTransport::new(Arc::new(NoopControl), receiver);

    assert!(matches!(
        transport.receive_line(Duration::from_millis(/*millis*/ 1)),
        Err(TransportReadError::Timeout)
    ));
    events.send(TransportEvent::Terminated(Some(9))).unwrap();
    assert!(matches!(
        transport.receive_line(Duration::from_millis(/*millis*/ 10)),
        Err(TransportReadError::Terminated(Some(9)))
    ));
}

struct NoopControl;

impl ProcessControl for NoopControl {
    fn write(&self, _message: &str) -> Result<(), String> {
        Ok(())
    }

    fn terminate(&self) -> Result<(), String> {
        Ok(())
    }
}
