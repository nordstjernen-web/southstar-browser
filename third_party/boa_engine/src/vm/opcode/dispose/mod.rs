use crate::{
    Context, JsResult, JsValue,
    builtins::disposable_stack::{
        DisposeCapability, DisposeHint, DisposeStep, add_dispose_error_value,
    },
    js_error,
    vm::opcode::{IndexOperand, Operation, RegisterOperand},
};

/// Reads the completion a disposal scope ends with from its `has_error` and `error` registers.
fn read_completion(
    has_error: RegisterOperand,
    error: RegisterOperand,
    context: &Context,
) -> Option<JsValue> {
    context
        .vm
        .get_register(has_error.into())
        .to_boolean()
        .then(|| context.vm.get_register(error.into()).clone())
}

/// Writes the completion of a disposal scope back to its `has_error` and `error` registers.
fn write_completion(
    has_error: RegisterOperand,
    error: RegisterOperand,
    completion: Option<JsValue>,
    context: &mut Context,
) {
    context
        .vm
        .set_register(has_error.into(), completion.is_some().into());
    if let Some(value) = completion {
        context.vm.set_register(error.into(), value);
    }
}

/// `CreateDisposeCapability` implements the Opcode Operation for `Opcode::CreateDisposeCapability`
///
/// Operation:
///  - Stores a new, empty `DisposeCapability` in `dst`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CreateDisposeCapability;

impl CreateDisposeCapability {
    #[inline(always)]
    pub(crate) fn operation(dst: RegisterOperand, context: &mut Context) {
        context
            .vm
            .set_register(dst.into(), DisposeCapability::new_object().into());
    }
}

impl Operation for CreateDisposeCapability {
    const NAME: &'static str = "CreateDisposeCapability";
    const INSTRUCTION: &'static str = "INST - CreateDisposeCapability";
    const COST: u8 = 2;
}

/// `AddDisposableResource` implements the Opcode Operation for `Opcode::AddDisposableResource`
///
/// Operation:
///  - Adds the value of a `using` (or, if `hint` is set, `await using`) binding to a capability.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AddDisposableResource;

impl AddDisposableResource {
    #[inline(always)]
    pub(crate) fn operation(
        (capability, value, hint): (RegisterOperand, RegisterOperand, IndexOperand),
        context: &mut Context,
    ) -> JsResult<()> {
        let capability = context
            .vm
            .get_register(capability.into())
            .as_object()
            .ok_or_else(|| js_error!(TypeError: "missing dispose capability"))?;
        let value = context.vm.get_register(value.into()).clone();
        let hint = DisposeHint::from_async(u32::from(hint) != 0);
        let mut cap = capability
            .downcast_mut::<DisposeCapability>()
            .ok_or_else(|| js_error!(TypeError: "missing dispose capability"))?;
        cap.add(value, hint, None, context)
    }
}

impl Operation for AddDisposableResource {
    const NAME: &'static str = "AddDisposableResource";
    const INSTRUCTION: &'static str = "INST - AddDisposableResource";
    const COST: u8 = 4;
}

/// `DisposeResources` implements the Opcode Operation for `Opcode::DisposeResources`
///
/// Operation:
///  - Disposes the synchronous resources of a capability, folding any errors into the
///    completion held by `has_error` and `error`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DisposeResources;

impl DisposeResources {
    #[inline(always)]
    pub(crate) fn operation(
        (capability, has_error, error): (RegisterOperand, RegisterOperand, RegisterOperand),
        context: &mut Context,
    ) {
        let Some(capability) = context.vm.get_register(capability.into()).as_object() else {
            return;
        };
        let mut completion = read_completion(has_error, error, context);
        DisposeCapability::dispose_sync(&capability, &mut completion, context);
        write_completion(has_error, error, completion, context);
    }
}

impl Operation for DisposeResources {
    const NAME: &'static str = "DisposeResources";
    const INSTRUCTION: &'static str = "INST - DisposeResources";
    const COST: u8 = 6;
}

/// `AsyncDisposeStep` implements the Opcode Operation for `Opcode::AsyncDisposeStep`
///
/// Operation:
///  - Disposes resources of a capability up to the next value to await, which it stores in
///    `value`; stores `true` in `done` when every resource has been disposed.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AsyncDisposeStep;

impl AsyncDisposeStep {
    #[inline(always)]
    pub(crate) fn operation(
        (capability, has_error, error, value, done): (
            RegisterOperand,
            RegisterOperand,
            RegisterOperand,
            RegisterOperand,
            RegisterOperand,
        ),
        context: &mut Context,
    ) {
        let Some(capability) = context.vm.get_register(capability.into()).as_object() else {
            context.vm.set_register(done.into(), true.into());
            return;
        };
        let mut completion = read_completion(has_error, error, context);
        let step = DisposeCapability::step(&capability, &mut completion, context);
        write_completion(has_error, error, completion, context);
        match step {
            DisposeStep::Done => context.vm.set_register(done.into(), true.into()),
            DisposeStep::Await(awaited) => {
                context.vm.set_register(done.into(), false.into());
                context.vm.set_register(value.into(), awaited);
            }
        }
    }
}

impl Operation for AsyncDisposeStep {
    const NAME: &'static str = "AsyncDisposeStep";
    const INSTRUCTION: &'static str = "INST - AsyncDisposeStep";
    const COST: u8 = 6;
}

/// `AddDisposeError` implements the Opcode Operation for `Opcode::AddDisposeError`
///
/// Operation:
///  - Folds the error a disposal threw into the completion held by `has_error` and `error`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AddDisposeError;

impl AddDisposeError {
    #[inline(always)]
    pub(crate) fn operation(
        (has_error, error, new_error): (RegisterOperand, RegisterOperand, RegisterOperand),
        context: &mut Context,
    ) {
        let mut completion = read_completion(has_error, error, context);
        let new_error = context.vm.get_register(new_error.into()).clone();
        add_dispose_error_value(&mut completion, new_error, context);
        write_completion(has_error, error, completion, context);
    }
}

impl Operation for AddDisposeError {
    const NAME: &'static str = "AddDisposeError";
    const INSTRUCTION: &'static str = "INST - AddDisposeError";
    const COST: u8 = 3;
}
