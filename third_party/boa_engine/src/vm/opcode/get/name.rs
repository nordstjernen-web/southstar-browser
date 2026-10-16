use boa_ast::scope::{BindingLocator, BindingLocatorScope};

use crate::{
    Context, JsResult, JsValue,
    error::JsNativeError,
    object::{internal_methods::InternalMethodPropertyContext, shape::slot::SlotAttributes},
    property::PropertyKey,
    vm::{
        LookedUp,
        opcode::{IndexOperand, Operation, RegisterOperand},
    },
};

/// `GetName` implements the Opcode Operation for `Opcode::GetName`
///
/// Operation:
///  - Find a binding on the environment chain and store its value in dst.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GetName;

impl GetName {
    #[inline(always)]
    pub(crate) fn operation(
        (value, index): (RegisterOperand, IndexOperand),
        context: &mut Context,
    ) -> JsResult<()> {
        let mut binding_locator =
            context.vm.frame().code_block.bindings[usize::from(index)].clone();
        context.find_runtime_binding(&mut binding_locator)?;
        let result = context.get_binding(&binding_locator)?.ok_or_else(|| {
            let name = binding_locator.name().to_std_string_escaped();
            JsNativeError::reference().with_message(format!("{name} is not defined"))
        })?;
        context.vm.set_register(value.into(), result);
        Ok(())
    }
}

impl Operation for GetName {
    const NAME: &'static str = "GetName";
    const INSTRUCTION: &'static str = "INST - GetName";
    const COST: u8 = 4;
}

/// `GetNameGlobal` implements the Opcode Operation for `Opcode::GetNameGlobal`
///
/// Operation:
///  - Find a binding in the global object and store its value in dst.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GetNameGlobal;

impl GetNameGlobal {
    #[inline(always)]
    pub(crate) fn operation(
        (dst, index, ic_index): (RegisterOperand, IndexOperand, IndexOperand),
        context: &mut Context,
    ) -> JsResult<()> {
        let mut binding_locator =
            context.vm.frame().code_block.bindings[usize::from(index)].clone();
        context.find_runtime_binding(&mut binding_locator)?;

        let result = if binding_locator.is_global() {
            get_global_object_binding(&binding_locator, ic_index, context)?
        } else {
            context.get_binding(&binding_locator)?.ok_or_else(|| {
                let name = binding_locator.name().to_std_string_escaped();
                JsNativeError::reference().with_message(format!("{name} is not defined"))
            })?
        };

        context.vm.set_register(dst.into(), result);
        Ok(())
    }
}

/// Reads the global object property a global binding names through the inline cache at
/// `ic_index`, throwing a `ReferenceError` when it does not exist.
fn get_global_object_binding(
    binding_locator: &BindingLocator,
    ic_index: IndexOperand,
    context: &mut Context,
) -> JsResult<JsValue> {
    let object = context.global_object();

    let ic = &context.vm.frame().code_block().ic[usize::from(ic_index)];

    let object_borrowed = object.borrow();
    if let Some((shape, slot)) = ic.get(object_borrowed.shape()) {
        let mut result = if slot.attributes.contains(SlotAttributes::PROTOTYPE) {
            let prototype = shape.prototype().expect("prototype should have value");
            let prototype = prototype.borrow();
            prototype.properties().storage[slot.index as usize].clone()
        } else {
            object_borrowed.properties().storage[slot.index as usize].clone()
        };

        drop(object_borrowed);
        if slot.attributes.has_get() && result.is_object() {
            result = result.as_object().expect("should contain getter").call(
                &object.clone().into(),
                &[],
                context,
            )?;
        }
        return Ok(result);
    }

    let looked_up = LookedUp::new(object_borrowed.shape());
    drop(object_borrowed);

    let key: PropertyKey = ic.name.clone().into();

    let context = &mut InternalMethodPropertyContext::new(context);
    let Some(result) = object.__try_get__(&key, object.clone().into(), context)? else {
        let name = binding_locator.name().to_std_string_escaped();
        return Err(JsNativeError::reference()
            .with_message(format!("{name} is not defined"))
            .into());
    };

    // Cache the property.
    let slot = *context.slot();
    if slot.is_cacheable() {
        let ic = &context.vm.frame().code_block.ic[usize::from(ic_index)];
        let object_borrowed = object.borrow();
        let shape = object_borrowed.shape();
        ic.set_after_access(&looked_up, shape, slot);
    }

    Ok(result)
}

impl Operation for GetNameGlobal {
    const NAME: &'static str = "GetNameGlobal";
    const INSTRUCTION: &'static str = "INST - GetNameGlobal";
    const COST: u8 = 4;
}

/// `GetLocator` implements the Opcode Operation for `Opcode::GetLocator`
///
/// Operation:
///  - Find a binding on the environment and set the `current_binding` of the current frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GetLocator;

impl GetLocator {
    #[inline(always)]
    pub(crate) fn operation(index: IndexOperand, context: &mut Context) -> JsResult<()> {
        let mut binding_locator =
            context.vm.frame().code_block.bindings[usize::from(index)].clone();
        context.find_runtime_binding(&mut binding_locator)?;

        // A strict reference that resolves to no binding stays unresolvable, so the `PutValue`
        // that follows throws even if evaluating the right-hand side created the property.
        let unresolvable = context.vm.frame().strict()
            && matches!(binding_locator.scope(), BindingLocatorScope::GlobalObject)
            && !context.is_initialized_binding(&binding_locator)?;

        context
            .vm
            .frame_mut()
            .binding_stack
            .push((binding_locator, unresolvable));

        Ok(())
    }
}

impl Operation for GetLocator {
    const NAME: &'static str = "GetLocator";
    const INSTRUCTION: &'static str = "INST - GetLocator";
    const COST: u8 = 4;
}

/// `GetNameAndLocator` implements the Opcode Operation for `Opcode::GetNameAndLocator`
///
/// Operation:
///  - Find a binding on the environment chain and store its value in dst, setting the
///    `current_binding` of the current frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GetNameAndLocator;

impl GetNameAndLocator {
    #[inline(always)]
    pub(crate) fn operation(
        (value, index): (RegisterOperand, IndexOperand),
        context: &mut Context,
    ) -> JsResult<()> {
        let mut binding_locator =
            context.vm.frame().code_block.bindings[usize::from(index)].clone();
        context.find_runtime_binding(&mut binding_locator)?;
        let result = context.get_binding(&binding_locator)?.ok_or_else(|| {
            let name = binding_locator.name().to_std_string_escaped();
            JsNativeError::reference().with_message(format!("{name} is not defined"))
        })?;

        context
            .vm
            .frame_mut()
            .binding_stack
            .push((binding_locator, false));
        context.vm.set_register(value.into(), result);
        Ok(())
    }
}

impl Operation for GetNameAndLocator {
    const NAME: &'static str = "GetNameAndLocator";
    const INSTRUCTION: &'static str = "INST - GetNameAndLocator";
    const COST: u8 = 4;
}

/// `GetNameAndLocatorGlobal` implements the Opcode Operation for
/// `Opcode::GetNameAndLocatorGlobal`
///
/// Operation:
///  - Like `GetNameAndLocator`, reading a global object binding through an inline cache.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GetNameAndLocatorGlobal;

impl GetNameAndLocatorGlobal {
    #[inline(always)]
    pub(crate) fn operation(
        (value, index, ic_index): (RegisterOperand, IndexOperand, IndexOperand),
        context: &mut Context,
    ) -> JsResult<()> {
        let mut binding_locator =
            context.vm.frame().code_block.bindings[usize::from(index)].clone();
        context.find_runtime_binding(&mut binding_locator)?;
        let result = if binding_locator.is_global() {
            get_global_object_binding(&binding_locator, ic_index, context)?
        } else {
            context.get_binding(&binding_locator)?.ok_or_else(|| {
                let name = binding_locator.name().to_std_string_escaped();
                JsNativeError::reference().with_message(format!("{name} is not defined"))
            })?
        };

        context
            .vm
            .frame_mut()
            .binding_stack
            .push((binding_locator, false));
        context.vm.set_register(value.into(), result);
        Ok(())
    }
}

impl Operation for GetNameAndLocatorGlobal {
    const NAME: &'static str = "GetNameAndLocatorGlobal";
    const INSTRUCTION: &'static str = "INST - GetNameAndLocatorGlobal";
    const COST: u8 = 4;
}

/// `GetNameOrUndefined` implements the Opcode Operation for `Opcode::GetNameOrUndefined`
///
/// Operation:
///  - Find a binding on the environment chain and store its value in dst. If the binding does not exist, store undefined.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GetNameOrUndefined;

impl GetNameOrUndefined {
    #[inline(always)]
    pub(crate) fn operation(
        (value, index): (RegisterOperand, IndexOperand),
        context: &mut Context,
    ) -> JsResult<()> {
        let mut binding_locator =
            context.vm.frame().code_block.bindings[usize::from(index)].clone();

        context.find_runtime_binding(&mut binding_locator)?;

        // The binding may resolve to the global object only at runtime, when an `eval`
        // binding that it was compiled against does not exist in this environment.
        let result = if let Some(value) = context.get_binding(&binding_locator)? {
            value
        } else if binding_locator.is_global() {
            JsValue::undefined()
        } else {
            let name = binding_locator.name().to_std_string_escaped();
            return Err(JsNativeError::reference()
                .with_message(format!("{name} is not defined"))
                .into());
        };

        context.vm.set_register(value.into(), result);
        Ok(())
    }
}

impl Operation for GetNameOrUndefined {
    const NAME: &'static str = "GetNameOrUndefined";
    const INSTRUCTION: &'static str = "INST - GetNameOrUndefined";
    const COST: u8 = 4;
}
