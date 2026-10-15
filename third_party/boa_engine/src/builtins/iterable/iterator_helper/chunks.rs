use std::{cell::Cell, collections::VecDeque};

use boa_gc::{Finalize, Trace};

use crate::{
    JsValue,
    builtins::{Array, iterable::IteratorRecord},
    native_function::{CoroutineBranch, CoroutineState, NativeCoroutine},
};

/// How [`Iterator.prototype.windows`][Windows] treats a final window shorter than `windowSize`.
#[derive(Clone, Copy, PartialEq, Eq, Trace, Finalize)]
#[boa_gc(empty_trace)]
pub(crate) enum Undersized {
    OnlyFull,
    AllowPartial,
}

/// [`Iterator.prototype.chunks ( chunkSize )`][spec] and
/// [`Iterator.prototype.windows ( windowSize [ , undersized ] )`][spec].
///
/// Groups the values of the underlying iterator into arrays: consecutive non-overlapping
/// chunks, or sliding windows that advance by one value.
///
/// [spec]: https://tc39.es/proposal-iterator-chunking/
#[derive(Trace, Finalize, Default)]
#[boa_gc(unsafe_no_drop)]
pub(crate) enum Chunks {
    #[default]
    Completed,
    Running {
        iterated: IteratorRecord,
        size: usize,
        windows: Option<Undersized>,
        buffer: VecDeque<JsValue>,
    },
}

impl Chunks {
    #[allow(
        clippy::new_ret_no_self,
        reason = "slightly cleaner to have this be a `new` method"
    )]
    pub(crate) fn new(
        iterated: IteratorRecord,
        size: usize,
        windows: Option<Undersized>,
    ) -> NativeCoroutine {
        NativeCoroutine::from_copy_closure_with_captures(
            |completion, state, context| {
                let Self::Running {
                    mut iterated,
                    size,
                    windows,
                    mut buffer,
                } = state.take()
                else {
                    return CoroutineState::Break(Ok(()));
                };

                // IfAbruptCloseIterator(completion, iterated).
                iterated.if_abrupt_close_iterator(completion, context)?;

                loop {
                    // i. Let value be ? IteratorStepValue(iterated).
                    let Some(value) = iterated.step_value(context).branch()? else {
                        // ii. If value is done, then
                        let partial = match windows {
                            None => !buffer.is_empty(),
                            Some(undersized) => {
                                undersized == Undersized::AllowPartial
                                    && !buffer.is_empty()
                                    && buffer.len() < size
                            }
                        };
                        if partial {
                            // 1. Perform ? Yield(CreateArrayFromList(buffer)).
                            let chunk = Array::create_array_from_list(buffer, context);
                            return CoroutineState::Continue(chunk.into());
                        }
                        // 2. Return ReturnCompletion(undefined).
                        return CoroutineState::Break(Ok(()));
                    };

                    if windows.is_some() && buffer.len() == size {
                        buffer.pop_front();
                    }
                    // iii. Append value to buffer.
                    buffer.push_back(value);

                    // iv. If the number of elements in buffer is size, then
                    if buffer.len() == size {
                        // 1. Let completion be Completion(Yield(CreateArrayFromList(buffer))).
                        let chunk = Array::create_array_from_list(buffer.iter().cloned(), context);
                        if windows.is_none() {
                            // 3. Set buffer to a new empty List.
                            buffer.clear();
                        }
                        state.set(Self::Running {
                            iterated,
                            size,
                            windows,
                            buffer,
                        });
                        return CoroutineState::Continue(chunk.into());
                    }
                }
            },
            Cell::new(Self::Running {
                iterated,
                size,
                windows,
                buffer: VecDeque::new(),
            }),
        )
    }
}
