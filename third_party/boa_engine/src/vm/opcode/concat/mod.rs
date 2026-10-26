use super::RegisterOperand;
use crate::{Context, JsResult, JsStr, JsString, vm::opcode::Operation};
use thin_vec::ThinVec;

/// `ConcatToString` implements the Opcode Operation for `Opcode::ConcatToString`
///
/// Operation:
///  - Concat multiple stack objects into a string.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ConcatToString;

impl ConcatToString {
    #[inline(always)]
    pub(super) fn operation(
        (string, values): (RegisterOperand, ThinVec<RegisterOperand>),

        context: &mut Context,
    ) -> JsResult<()> {
        let mut strings = Vec::with_capacity(values.len());
        for value in values {
            let val = context.vm.get_register(value.into()).clone();
            strings.push(val.to_string(context)?);
        }
        let s = concat_strings(&strings);
        context.vm.set_register(string.into(), s.into());
        Ok(())
    }
}

/// Concatenates `strings`, joining the long ones as ropes so that a template literal which
/// appends to a long string, such as `` s = `${s}<li>` ``, does not copy it.
fn concat_strings(strings: &[JsString]) -> JsString {
    const LONG: usize = 512;
    fn as_strs(strings: &[JsString]) -> Vec<JsStr<'_>> {
        strings.iter().map(JsString::as_str).collect()
    }
    if strings.iter().all(|s| s.len() < LONG) {
        return JsString::concat_array(&as_strs(strings));
    }
    let mut result = JsString::default();
    let mut run_start = 0;
    for (i, string) in strings.iter().enumerate() {
        if string.len() >= LONG {
            let run = JsString::concat_array(&as_strs(&strings[run_start..i]));
            result = JsString::concat_lazy(&JsString::concat_lazy(&result, &run), string);
            run_start = i + 1;
        }
    }
    let run = JsString::concat_array(&as_strs(&strings[run_start..]));
    JsString::concat_lazy(&result, &run)
}

impl Operation for ConcatToString {
    const NAME: &'static str = "ConcatToString";
    const INSTRUCTION: &'static str = "INST - ConcatToString";
    const COST: u8 = 6;
}
