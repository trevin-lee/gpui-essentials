//! The component library the web renders with, as lang-core's parser sees it: each component's
//! positional parameters with their JSON schemas, and its two-way ($binding) props. It's read from
//! library.json, which tests/openui/golden.mjs writes from @openuidev/react-ui itself, so the
//! parameters here are the library's, in its order.

use super::value::{Obj, V};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

/// The JSON schema keywords lang-core's validation reads. golden.mjs refuses to write a schema
/// with any other keyword, so nothing here is silently ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Schema {
    #[serde(rename = "type")]
    ty: Option<TypeSpec>,
    #[serde(rename = "enum")]
    pub one_of: Option<Vec<serde_json::Value>>,
    #[serde(rename = "const")]
    pub constant: Option<serde_json::Value>,
    pub properties: Option<Vec<(String, Schema)>>,
    pub required: Option<Vec<String>>,
    pub items: Option<Box<Schema>>,
    #[serde(rename = "anyOf")]
    pub any_of: Option<Vec<Schema>>,
    #[serde(rename = "$ref")]
    pub reference: Option<String>,
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TypeSpec {
    One(String),
    Many(#[allow(dead_code)] Vec<String>),
}

impl Schema {
    /// The schema's type when it names one ("string", "object"...); a list of types names none.
    pub fn ty(&self) -> Option<&str> {
        match &self.ty {
            Some(TypeSpec::One(t)) => Some(t),
            _ => None,
        }
    }

    pub fn default_value(&self) -> Option<V> {
        self.default.as_ref().map(json_value)
    }

    /// The component a `{"$ref": "#/$defs/Name"}` names.
    pub fn ref_name(&self) -> Option<&str> {
        self.reference
            .as_deref()
            .map(|r| r.rsplit('/').next().unwrap_or(r))
    }

    /// $ref, anyOf or oneOf: a shape a plain value can't be matched against with certainty.
    pub fn composite(&self) -> bool {
        self.reference.is_some() || self.any_of.is_some()
    }
}

/// A JSON value from the library (a default, an enum member) as a program value.
pub(crate) fn json_value(j: &serde_json::Value) -> V {
    use serde_json::Value as J;
    match j {
        J::Null => V::Null,
        J::Bool(b) => V::Bool(*b),
        J::Number(n) => V::Num(n.as_f64().unwrap_or(0.0)),
        J::String(s) => V::str(s),
        J::Array(a) => V::arr(a.iter().map(json_value).collect()),
        J::Object(m) => {
            let mut o = Obj::new();
            for (k, v) in m {
                o.set(k.as_str(), json_value(v));
            }
            V::obj(o)
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Param {
    pub name: String,
    pub required: bool,
    pub schema: Schema,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Component {
    pub params: Vec<Param>,
    #[serde(default)]
    pub reactive: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Library {
    pub root: String,
    pub components: HashMap<String, Component>,
}

impl Library {
    pub fn get(&self, name: &str) -> Option<&Component> {
        self.components.get(name)
    }

    pub fn has(&self, name: &str) -> bool {
        self.components.contains_key(name)
    }

    /// Whether the component's prop is a two-way binding ($binding<...> in the reference).
    pub fn reactive(&self, component: &str, prop: &str) -> bool {
        self.components
            .get(component)
            .is_some_and(|c| c.reactive.iter().any(|p| p == prop))
    }
}

pub(crate) static LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    serde_json::from_str(include_str!("library.json"))
        .expect("library.json is the library golden.mjs wrote")
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_is_the_reference_s() {
        let lib = &*LIBRARY;
        assert_eq!(lib.root, "Stack");
        let names: Vec<&str> = lib
            .get("Button")
            .unwrap()
            .params
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["label", "action", "variant", "type", "size"]);
        assert!(
            lib.reactive("Modal", "open")
                && lib.reactive("Input", "value")
                && !lib.reactive("Input", "name")
        );
        let text = lib.get("Text").unwrap();
        assert_eq!(
            text.params[3]
                .schema
                .default_value()
                .and_then(|v| v.as_str().map(str::to_string))
                .as_deref(),
            Some("text")
        );
        // Every component the model draws is in the library, so none of them is read as unknown.
        for name in [
            "Stack",
            "Card",
            "Tabs",
            "TabItem",
            "Accordion",
            "AccordionItem",
            "Steps",
            "StepsItem",
            "Carousel",
            "Separator",
            "Modal",
            "CardHeader",
            "TextContent",
            "MarkDownRenderer",
            "Callout",
            "TextCallout",
            "Image",
            "ImageBlock",
            "ImageGallery",
            "CodeBlock",
            "InlineHeader",
            "Table",
            "Col",
            "EditableTable",
            "BarChart",
            "HorizontalBarChart",
            "LineChart",
            "AreaChart",
            "RadarChart",
            "PieChart",
            "RadialChart",
            "SingleStackedBarChart",
            "ScatterChart",
            "Form",
            "FormControl",
            "Input",
            "TextArea",
            "Select",
            "DatePicker",
            "Slider",
            "CheckBoxGroup",
            "RadioGroup",
            "SwitchGroup",
            "Chips",
            "OptionCards",
            "Label",
            "Button",
            "Buttons",
            "IconButton",
            "TagBlock",
            "Tag",
            "Icon",
            "EntityList",
            "ListBlock",
            "ListItem",
            "SnippetCardBlock",
            "OverviewCardBlock",
            "ContextCardBlock",
            "CompositeCardBlock",
            "VisualCardBlock",
            "Text",
            "BoldText",
            "IconText",
            "ImageText",
            "ImageTextLarge",
            "MetricIndicatorInline",
            "MetricIndicatorWithStrikethrough",
        ] {
            assert!(lib.has(name), "{name} is drawn but not in library.json");
        }
    }
}
