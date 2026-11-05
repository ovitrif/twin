//! The iPhone's on-screen elements, read through the accessibility audit
//! daemon that Xcode's Accessibility Inspector uses. Nothing is installed on
//! the phone and no screenshot is taken: the daemon moves its inspector
//! cursor through the front app's elements, one at a time, and reports each.
//!
//! The daemon speaks DTX (the Instruments protocol) behind an RSD shim. Its
//! values come wrapped as `{ObjectType, Value}` dictionaries, and attributes
//! are read by handing back the attribute objects it listed for an element.

use idevice::{
    Idevice,
    dvt::{message::AuxValue, remote_server::RemoteServerClient},
    rsd::RsdHandshake,
};
use plist::{Dictionary, Value};
use serde::Serialize;
use std::time::Duration;

const SERVICE: &str = "com.apple.accessibility.axAuditDaemon.remoteserver.shim.remote";
/// Inspector cursor moves
const NEXT: i64 = 4;
const FIRST: i64 = 5;
/// A screen with more elements than this is cut off, as a guard against a
/// cursor that never wraps
const MAX_ELEMENTS: usize = 400;

/// One element as agents see it
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Element {
    pub index: usize,
    pub label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub traits: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub identifier: String,
    /// What VoiceOver would say
    pub caption: String,
    #[serde(skip)]
    pub handle: Vec<u8>,
}

struct Focus {
    element: Element,
    /// The element as the daemon sent it, to hand back for attributes and actions
    raw: Value,
    /// Attribute objects by name, from the element's inspector sections
    attributes: Vec<(String, Value)>,
}

pub struct Inspector {
    server: RemoteServerClient<Box<dyn idevice::ReadWrite>>,
}

fn passthrough(v: Value) -> Value {
    let mut d = Dictionary::new();
    d.insert("ObjectType".into(), "passthrough".into());
    d.insert("Value".into(), v);
    Value::Dictionary(d)
}

/// Strips the `{ObjectType, Value}` wrappers
fn unwrap(v: &Value) -> &Value {
    let mut v = v;
    while let Some(next) = v.as_dictionary().and_then(|d| d.get("Value")) {
        v = next;
    }
    v
}

fn field<'a>(d: &'a Dictionary, key: &str) -> Option<&'a Value> {
    d.get(key).map(unwrap)
}

fn text(v: Option<&Value>) -> String {
    match v.map(unwrap) {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|i| text(Some(i)))
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Some(Value::Integer(n)) => n.to_string(),
        Some(Value::Real(n)) => n.to_string(),
        Some(Value::Boolean(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// Reads one `hostInspectorCurrentElementChanged:` payload
fn parse_focus(v: &Value, index: usize) -> Option<Focus> {
    let d = unwrap(v).as_dictionary()?;
    let raw = d.get("ElementValue_v1")?.clone();
    let element = unwrap(&raw).as_dictionary()?;
    let handle = field(element, "PlatformElementValue_v1")?
        .as_data()?
        .to_vec();
    let mut attributes = vec![];
    for section in field(d, "InspectorSectionsValue_v1")
        .and_then(|s| s.as_array())
        .into_iter()
        .flatten()
    {
        let Some(section) = unwrap(section).as_dictionary() else {
            continue;
        };
        for attribute in field(section, "ElementAttributesValue_v1")
            .and_then(|a| a.as_array())
            .into_iter()
            .flatten()
        {
            if let Some(name) = unwrap(attribute)
                .as_dictionary()
                .and_then(|a| field(a, "AttributeNameValue_v1"))
                .and_then(|n| n.as_string())
            {
                attributes.push((name.to_string(), attribute.clone()));
            }
        }
    }
    Some(Focus {
        element: Element {
            index,
            label: String::new(),
            value: String::new(),
            traits: String::new(),
            identifier: text(field(element, "AccessibilityIdentifier_v1")),
            caption: text(field(d, "CaptionTextValue_v1")),
            handle,
        },
        raw,
        attributes,
    })
}

impl Inspector {
    pub async fn connect(
        adapter: &mut idevice::tcp::handle::AdapterHandle,
        handshake: &RsdHandshake,
    ) -> Result<Self, String> {
        let port = handshake
            .services
            .get(SERVICE)
            .ok_or("this iPhone has no accessibility inspector service")?
            .port;
        let stream = adapter
            .connect(port)
            .await
            .map_err(|e| format!("inspector: {e:?}"))?;
        let mut device = Idevice::new(Box::new(stream), "twin");
        device
            .rsd_checkin()
            .await
            .map_err(|e| format!("inspector check-in: {e:?}"))?;
        let server = RemoteServerClient::new(device.get_socket().ok_or("inspector: no socket")?);
        let mut inspector = Self { server };
        // The daemon opens with its capabilities and API version; it ignores
        // the inspector until those are read
        for _ in 0..2 {
            if tokio::time::timeout(Duration::from_millis(500), inspector.server.read_message(0))
                .await
                .is_err()
            {
                break;
            }
        }
        inspector.call("deviceCapabilities", vec![]).await?;
        inspector
            .send("deviceSetAppMonitoringEnabled:", vec![Value::Boolean(true)])
            .await?;
        inspector
            .send("deviceInspectorShowVisuals:", vec![Value::Boolean(false)])
            .await?;
        inspector
            .send(
                "deviceInspectorSetMonitoredEventType:",
                vec![Value::Integer(0.into())],
            )
            .await?;
        Ok(inspector)
    }

    async fn send(&mut self, method: &str, args: Vec<Value>) -> Result<(), String> {
        let args = args.into_iter().map(AuxValue::archived_value).collect();
        self.server
            .call_method(0, Some(Value::String(method.into())), Some(args), false)
            .await
            .map_err(|e| format!("{method} {e:?}"))
    }

    /// Calls a method and returns its reply, skipping the daemon's own events
    async fn call(&mut self, method: &str, args: Vec<Value>) -> Result<Option<Value>, String> {
        let args = args.into_iter().map(AuxValue::archived_value).collect();
        self.server
            .call_method(0, Some(Value::String(method.into())), Some(args), true)
            .await
            .map_err(|e| format!("{method} {e:?}"))?;
        let reply = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let m = self
                    .server
                    .read_message(0)
                    .await
                    .map_err(|e| format!("{method} {e:?}"))?;
                let event = m
                    .data
                    .as_ref()
                    .and_then(|d| d.as_string())
                    .is_some_and(|n| n.starts_with("host"));
                if !event {
                    return Ok::<_, String>(m.data);
                }
            }
        })
        .await
        .map_err(|_| format!("{method}: no answer"))??;
        if reply
            .as_ref()
            .and_then(|r| r.as_dictionary())
            .is_some_and(|d| d.contains_key("NSCode"))
        {
            return Err(format!("{method}: the phone refused it"));
        }
        Ok(reply)
    }

    /// Moves the cursor and waits for the element it lands on; `None` when
    /// it did not move (already there, or nothing to move to)
    async fn step(
        &mut self,
        direction: i64,
        index: usize,
        wait: Duration,
    ) -> Result<Option<Focus>, String> {
        let mut options = Dictionary::new();
        options.insert("allowNonAX".into(), passthrough(0.into()));
        options.insert("includeContainers".into(), passthrough(0.into()));
        options.insert("direction".into(), passthrough(direction.into()));
        self.send(
            "deviceInspectorMoveWithOptions:",
            vec![passthrough(Value::Dictionary(options))],
        )
        .await?;
        let event = tokio::time::timeout(wait, async {
            loop {
                let m = self
                    .server
                    .read_message(0)
                    .await
                    .map_err(|e| format!("inspector: {e:?}"))?;
                if m.data.as_ref().and_then(|d| d.as_string())
                    == Some("hostInspectorCurrentElementChanged:")
                {
                    return Ok::<_, String>(m);
                }
            }
        })
        .await;
        let Ok(event) = event else { return Ok(None) };
        let event = event?;
        let Some(AuxValue::Array(bytes)) = event.aux.as_ref().and_then(|a| a.values.first()) else {
            return Ok(None);
        };
        let value = ns_keyed_archive::decode::from_bytes(bytes)
            .map_err(|e| format!("inspector event: {e:?}"))?;
        Ok(parse_focus(&value, index))
    }

    async fn attribute(&mut self, focus: &Focus, name: &str) -> String {
        let Some((_, attribute)) = focus.attributes.iter().find(|(n, _)| n == name) else {
            return String::new();
        };
        let attribute = attribute.clone();
        match self
            .call(
                "deviceElement:valueForAttribute:",
                vec![focus.raw.clone(), attribute],
            )
            .await
        {
            Ok(Some(v)) => text(Some(&v)),
            _ => String::new(),
        }
    }

    /// Walks the front app's elements from the first until the cursor comes
    /// back around, then moves it to the first again so a scrolled list
    /// returns to its top. List cells are reused as a list scrolls, so an
    /// element is the same one only when its handle and caption both match.
    pub async fn elements(&mut self) -> Result<Vec<Element>, String> {
        let quick = Duration::from_millis(700);
        // "First" moves nothing when the cursor is already there, or when a
        // finished session left it nowhere; "next" then lands on the second
        // element or the first, and a second "first" tells which
        let mut first = self.step(FIRST, 0, quick).await?;
        if first.is_none() {
            let next = self.step(NEXT, 0, Duration::from_millis(1500)).await?;
            first = match self.step(FIRST, 0, quick).await? {
                Some(f) => Some(f),
                None => next,
            };
        }
        let mut seen: Vec<Element> = vec![];
        let mut current = first;
        while let Some(mut focus) = current.take() {
            let repeat = seen
                .iter()
                .any(|e| e.handle == focus.element.handle && e.caption == focus.element.caption);
            if repeat || seen.len() >= MAX_ELEMENTS {
                break;
            }
            focus.element.label = self.attribute(&focus, "Label").await;
            focus.element.value = self.attribute(&focus, "Value").await;
            focus.element.traits = self.attribute(&focus, "TraitsHumanReadable").await;
            if focus.element.label.is_empty() {
                focus.element.label = focus.element.caption.clone();
            }
            seen.push(focus.element);
            current = self
                .step(NEXT, seen.len(), Duration::from_millis(1500))
                .await?;
        }
        // Back to the start; the second element is usually the top of the
        // content, so stopping there scrolls a long list back up
        if seen.len() > 1 {
            self.step(FIRST, 0, quick).await?;
            self.step(NEXT, 0, quick).await?;
            self.step(FIRST, 0, quick).await?;
        }
        Ok(seen)
    }

    /// Leaves the phone's inspector as it was
    pub async fn finish(mut self) {
        let _ = self
            .send(
                "deviceSetAppMonitoringEnabled:",
                vec![Value::Boolean(false)],
            )
            .await;
        let _ = self
            .send("deviceInspectorEnable:", vec![Value::Boolean(false)])
            .await;
        let _ = self
            .server
            .call_method(
                0,
                Some(Value::String("devicePerformFinalCleanup".into())),
                None,
                false,
            )
            .await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap(kind: &str, v: Value) -> Value {
        let mut d = Dictionary::new();
        d.insert("ObjectType".into(), kind.into());
        d.insert("Value".into(), v);
        Value::Dictionary(d)
    }

    fn dict(pairs: Vec<(&str, Value)>) -> Value {
        Value::Dictionary(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    #[test]
    fn reads_a_focus_event() {
        let attribute = |name: &str| {
            wrap(
                "AXAuditElementAttribute_v1",
                passthrough(dict(vec![(
                    "AttributeNameValue_v1",
                    passthrough(name.into()),
                )])),
            )
        };
        let section = wrap(
            "AXAuditInspectorSection_v1",
            passthrough(dict(vec![(
                "ElementAttributesValue_v1",
                passthrough(Value::Array(vec![
                    attribute("Label"),
                    attribute("AXAction-2010"),
                ])),
            )])),
        );
        let element = wrap(
            "AXAuditElement_v1",
            passthrough(dict(vec![
                (
                    "PlatformElementValue_v1",
                    passthrough(Value::Data(vec![1, 2, 3])),
                ),
                (
                    "AccessibilityIdentifier_v1",
                    passthrough("ok-button".into()),
                ),
            ])),
        );
        let event = wrap(
            "AXAuditInspectorFocus_v1",
            passthrough(dict(vec![
                ("CaptionTextValue_v1", passthrough("OK, Button".into())),
                ("ElementValue_v1", element),
                (
                    "InspectorSectionsValue_v1",
                    passthrough(Value::Array(vec![section])),
                ),
            ])),
        );
        let focus = parse_focus(&event, 3).unwrap();
        assert_eq!(focus.element.caption, "OK, Button");
        assert_eq!(focus.element.identifier, "ok-button");
        assert_eq!(focus.element.handle, vec![1, 2, 3]);
        assert_eq!(focus.element.index, 3);
        let names: Vec<_> = focus.attributes.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Label", "AXAction-2010"]);
    }

    #[test]
    fn element_json_skips_empty_fields() {
        let e = Element {
            index: 0,
            label: "Send".into(),
            value: String::new(),
            traits: "Button".into(),
            identifier: String::new(),
            caption: "Send, Button".into(),
            handle: vec![9],
        };
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"index":0,"label":"Send","traits":"Button","caption":"Send, Button"}"#
        );
    }
}
