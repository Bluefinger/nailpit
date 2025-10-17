use std::sync::Arc;

use bytes::BytesMut;
use winnow::{
    ModalResult, Parser,
    ascii::{Caseless, alpha1, space0},
    combinator::alt,
    error::ErrMode,
    token::{literal, rest, take_until},
};

fn process_template<'any>(template: &'any mut &str) -> ModalResult<(&'any str, TemplateState)> {
    match take_until(0.., "{{").parse_next(template) {
        Ok(part) => {
            literal("{{").parse_next(template)?;

            space0(template)?;

            let mut placeholder = alpha1(template)?;

            space0(template)?;

            literal("}}").parse_next(template)?;

            let placeholder = &mut placeholder;

            let state = alt((
                Caseless("title").map(|_| (part, TemplateState::Title)),
                Caseless("initial").map(|_| (part, TemplateState::Initial)),
                Caseless("main").map(|_| (part, TemplateState::Main)),
                Caseless("extra").map(|_| (part, TemplateState::Extra)),
                Caseless("footer").map(|_| (part, TemplateState::Footer)),
            ))
            .parse_next(placeholder)?;

            rest.verify(|remaining: &str| remaining.is_empty())
                .parse_next(placeholder)?;

            Ok(state)
        }
        Err(ErrMode::Backtrack(_)) => Ok((template, TemplateState::Finished)),
        Err(e) => Err(e),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Template {
    Warning(WarningTemplate),
    Generated(GeneratedTemplate),
}

impl Template {
    pub fn get_template(&self) -> Arc<str> {
        match self {
            Self::Warning(index_template) => index_template.template.clone(),
            Self::Generated(generated_template) => generated_template.template.clone(),
        }
    }

    pub fn get_static_content(&self) -> Option<Arc<str>> {
        match self {
            Self::Warning(index_template) => Some(index_template.static_content.clone()),
            Self::Generated(_) => None,
        }
    }
}

impl From<WarningTemplate> for Template {
    fn from(value: WarningTemplate) -> Self {
        Self::Warning(value)
    }
}

impl From<GeneratedTemplate> for Template {
    fn from(value: GeneratedTemplate) -> Self {
        Self::Generated(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WarningTemplate {
    template: Arc<str>,
    static_content: Arc<str>,
}

impl WarningTemplate {
    pub fn init(template: Arc<str>, static_content: Arc<str>) -> Result<Self, TemplateError> {
        let cursor = TemplateCursor::new(template);

        if cursor.validate(&[
            TemplateState::Title,
            TemplateState::Main,
            TemplateState::Footer,
        ]) {
            Ok(Self {
                template: cursor.finish(),
                static_content,
            })
        } else {
            Err(TemplateError)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GeneratedTemplate {
    template: Arc<str>,
}

impl GeneratedTemplate {
    pub fn init(template: Arc<str>) -> Result<Self, TemplateError> {
        let cursor = TemplateCursor::new(template);

        if cursor.validate(&[
            TemplateState::Title,
            TemplateState::Initial,
            TemplateState::Main,
            TemplateState::Extra,
            TemplateState::Footer,
        ]) {
            Ok(Self {
                template: cursor.finish(),
            })
        } else {
            Err(TemplateError)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateError;

impl core::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid template, contains disallowed placeholders")
    }
}

impl core::error::Error for TemplateError {}

pub struct TemplateCursor {
    cursor: *const str,
    _template: Arc<str>,
}

impl TemplateCursor {
    pub fn new(template: Arc<str>) -> Self {
        Self {
            cursor: Arc::as_ptr(&template),
            _template: template,
        }
    }

    fn finish(self) -> Arc<str> {
        self._template
    }

    fn cast_cursor(&self) -> &str {
        // SAFETY: The pointer will always be valid as we own an Arc instance
        // within the cursor struct, meaning as long as the cursor lives, the
        // Arc will never be dropped and cleaned.
        unsafe { &*self.cursor }
    }

    /// Advances the template cursor to the next placeholder point, writing the
    /// portion of the static template to the buffer before returning the placeholder
    /// reached by the cursor. If the template has no placeholders, the entire
    /// template will be written to the buffer and the state returned will be
    /// [`TemplateState::Finished`].
    pub fn write_template(&mut self, buffer: &mut BytesMut) -> TemplateState {
        let template = &mut self.cast_cursor();

        match process_template(template) {
            Ok((part, state)) => {
                buffer.extend_from_slice(part.as_bytes());

                self.cursor = *template;

                state
            }
            Err(_) => TemplateState::Finished,
        }
    }

    fn validate(&self, included: &[TemplateState]) -> bool {
        let template = &mut self.cast_cursor();

        loop {
            match process_template(template) {
                Ok((_, state)) => {
                    if state == TemplateState::Finished {
                        return true;
                    }

                    if !included.contains(&state) {
                        return false;
                    }
                }
                Err(_) => return false,
            }
        }
    }
}

// SAFETY: TemplateCursor carries an Arc reference to the str allocated on the heap.
// This means the pointer will never be invalidated as long as the Arc instance lives,
// and as it is immutable, the allocation will never be changed.
unsafe impl Send for TemplateCursor {}
// SAFETY: TemplateCursor carries an Arc reference to the str allocated on the heap.
// This means the pointer will never be invalidated as long as the Arc instance lives,
// and as it is immutable, the allocation will never be changed.
unsafe impl Sync for TemplateCursor {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateState {
    Title,
    Initial,
    Main,
    Extra,
    Footer,
    Finished,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_placeholders() -> color_eyre::Result<()> {
        let mut template = "<h1>{{ TITLE }}</h1><p>{{main}}</p>";

        let template = &mut template;

        let (extracted, state) = process_template(template)
            .map_err(|a| color_eyre::eyre::eyre!("Template parsing error:\n{a}"))?;

        assert_eq!(extracted, "<h1>");
        assert_eq!(state, TemplateState::Title);
        assert_eq!(*template, "</h1><p>{{main}}</p>");

        let (extracted, state) = process_template(template)
            .map_err(|a| color_eyre::eyre::eyre!("Template parsing error:\n{a}"))?;

        assert_eq!(extracted, "</h1><p>");
        assert_eq!(state, TemplateState::Main);
        assert_eq!(*template, "</p>");

        let (extracted, state) = process_template(template)
            .map_err(|a| color_eyre::eyre::eyre!("Template parsing error:\n{a}"))?;

        assert_eq!(extracted, "</p>");
        assert_eq!(state, TemplateState::Finished);
        assert_eq!(*template, "</p>");

        Ok(())
    }

    #[test]
    fn writes_template_to_buffer() {
        let template: Arc<str> = Arc::from("<h1>{{ TITLE }}</h1><p>{{ MAIN }}</p>");

        let mut cursor = TemplateCursor::new(template);

        let mut buffer = BytesMut::new();

        loop {
            match cursor.write_template(&mut buffer) {
                TemplateState::Finished => {
                    break;
                }
                TemplateState::Title | TemplateState::Main => {
                    buffer.extend_from_slice(b"aaa");
                }
                state => panic!("INVALID STATE: {state:?}"),
            }
        }

        assert_eq!(buffer.as_ref(), b"<h1>aaa</h1><p>aaa</p>");
    }

    #[test]
    fn writes_static_template_to_buffer() {
        let template: Arc<str> = Arc::from("<h1>Static</h1><p>Template</p>");

        let mut cursor = TemplateCursor::new(template);

        let mut buffer = BytesMut::new();

        let state = cursor.write_template(&mut buffer);

        assert_eq!(state, TemplateState::Finished);
        assert_eq!(buffer.as_ref(), b"<h1>Static</h1><p>Template</p>");
    }

    #[test]
    fn wont_write_empty_template_to_buffer() {
        let template: Arc<str> = Arc::from("");

        let mut cursor = TemplateCursor::new(template);

        let mut buffer = BytesMut::new();

        let state = cursor.write_template(&mut buffer);

        assert_eq!(state, TemplateState::Finished);
        assert!(buffer.is_empty());
    }

    #[test]
    fn validates_templates_with_placeholders() {
        let template: Arc<str> = Arc::from("<h1>{{ TITLE }}</h1><p>{{ MAIN }}</p>");

        let generated_template = GeneratedTemplate::init(template.clone());

        assert_eq!(generated_template, Ok(GeneratedTemplate { template }));
    }

    #[test]
    fn validates_templates_with_invalid_placeholders() {
        // Partial matches should still be invalid
        let template: Arc<str> = Arc::from("<h1>{{ TITLE }}</h1><p>{{ MAINZ }}</p>");

        let generated_template = GeneratedTemplate::init(template);

        assert_eq!(generated_template, Err(TemplateError));

        let template: Arc<str> = Arc::from("<h1>{{ TITLE }}</h1><p>{{ huurrrr }}</p>");

        let generated_template = GeneratedTemplate::init(template);

        assert_eq!(generated_template, Err(TemplateError));
    }

    #[test]
    fn validates_static_templates() {
        // Static templates are also valid
        let template: Arc<str> = Arc::from("<h1>Static</h1><p>Template</p>");

        let generated_template = GeneratedTemplate::init(template.clone());

        assert_eq!(generated_template, Ok(GeneratedTemplate { template }));
    }

    #[test]
    fn validates_empty_templates() {
        // Empty templates should be valid, since they can be handled
        let template: Arc<str> = Arc::from("");

        let generated_template = GeneratedTemplate::init(template.clone());

        assert_eq!(generated_template, Ok(GeneratedTemplate { template }));
    }
}
