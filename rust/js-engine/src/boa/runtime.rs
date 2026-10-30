//! Southstar — runtimes, contexts and their handles on Boa: a runtime owns one Context, a context handle names one of its realms, and a scope re-enters that realm on demand.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell, UnsafeCell};
use core::ffi::c_void;
use core::mem::ManuallyDrop;
use core::ptr;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use boa_engine::builtins::promise::OperationType;
use boa_engine::context::HostHooks;
use boa_engine::job::{GenericJob, Job, JobExecutor, NativeAsyncJob, PromiseJob, TimeoutJob};
use boa_engine::module::{
    ModuleLoader, ModuleRequest, Referrer, SimpleModuleLoader, SyntheticModuleInitializer,
};
use boa_engine::object::builtins::{JsFunction, JsPromise};
use boa_engine::prelude::{Finalize, JsData, Trace};
use boa_engine::realm::Realm;
use boa_engine::{Context, JsError, JsNativeError, JsObject, JsResult, JsString, JsValue, Module};

use super::{Compiled, Scope, Value};
use crate::{ContextHandle, JobOutcome, MemoryUsage, RuntimeHandle, RuntimeOptions};

const CALL_DEPTH_LIMIT: usize = 10_000;
const VALUE_STACK_LIMIT: usize = 1 << 20;

pub(super) struct BoaRuntime {
    ctx: UnsafeCell<Context>,
    options: RuntimeOptions,
    opaque: Cell<*mut c_void>,
    executor: Rc<Executor>,
    loader: Rc<Loader>,
    records: RefCell<Vec<usize>>,
}

pub(super) struct RealmRecord {
    runtime: Weak<BoaRuntime>,
    realm: Realm,
    opaque: Cell<*mut c_void>,
}

#[derive(Trace, Finalize, JsData)]
struct RealmTag(usize);

type Registry<T> = ManuallyDrop<RefCell<HashMap<usize, Rc<T>>>>;

thread_local! {
    static RUNTIMES: Registry<BoaRuntime> = ManuallyDrop::new(RefCell::new(HashMap::new()));
    static RECORDS: Registry<RealmRecord> = ManuallyDrop::new(RefCell::new(HashMap::new()));
    static CONTEXTS: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
}

fn registered<T>(
    registry: &'static std::thread::LocalKey<Registry<T>>,
    key: usize,
) -> Option<Rc<T>> {
    registry.with(|map| map.borrow().get(&key).cloned())
}

fn unregister<T>(
    registry: &'static std::thread::LocalKey<Registry<T>>,
    key: usize,
) -> Option<Rc<T>> {
    registry.with(|map| map.borrow_mut().remove(&key))
}

#[derive(Default)]
struct Executor {
    promise_jobs: RefCell<VecDeque<PromiseJob>>,
    generic_jobs: RefCell<VecDeque<GenericJob>>,
    async_jobs: RefCell<VecDeque<NativeAsyncJob>>,
    timeout_jobs: RefCell<VecDeque<TimeoutJob>>,
    wait: bool,
    deadlines: RefCell<Vec<(Instant, TimeoutJob)>>,
}

enum Ran {
    Nothing,
    Job(Option<Realm>, JsResult<()>),
}

impl Executor {
    fn is_empty(&self) -> bool {
        self.promise_jobs.borrow().is_empty()
            && self.generic_jobs.borrow().is_empty()
            && self.async_jobs.borrow().is_empty()
            && self.timeout_jobs.borrow().is_empty()
    }

    fn front_realm(&self) -> Option<Realm> {
        if let Some(job) = self.promise_jobs.borrow().front() {
            return job.realm().cloned();
        }
        if let Some(job) = self.generic_jobs.borrow().front() {
            return Some(job.realm().clone());
        }
        self.async_jobs
            .borrow()
            .front()
            .and_then(|job| job.realm().cloned())
    }

    fn run_promise_job(&self, ctx: &mut Context) -> Option<Ran> {
        let job = self.promise_jobs.borrow_mut().pop_front()?;
        let realm = job.realm().cloned();
        Some(Ran::Job(realm, job.call(ctx).map(drop)))
    }

    fn run_one(&self, ctx: &mut Context) -> Ran {
        let async_job = self.async_jobs.borrow_mut().pop_front();
        if let Some(job) = async_job {
            let realm = job.realm().cloned();
            return Ran::Job(realm, self.drive(job, ctx));
        }
        if let Some(ran) = self.run_promise_job(ctx) {
            return ran;
        }
        let generic = self.generic_jobs.borrow_mut().pop_front();
        if let Some(job) = generic {
            let realm = Some(job.realm().clone());
            return Ran::Job(realm, job.call(ctx).map(drop));
        }
        let timeout = self.timeout_jobs.borrow_mut().pop_front();
        if let Some(job) = timeout {
            return Ran::Job(None, job.call(ctx).map(drop));
        }
        Ran::Nothing
    }

    fn drive(&self, job: NativeAsyncJob, ctx: &mut Context) -> JsResult<()> {
        use core::future::Future;
        use core::task::{Context as TaskContext, Poll, Waker};
        let cell = RefCell::new(ctx);
        let mut future = job.call(&cell);
        let waker = Waker::noop();
        let mut task = TaskContext::from_waker(waker);
        loop {
            if let Poll::Ready(result) = core::pin::Pin::new(&mut future).poll(&mut task) {
                return result.map(drop);
            }
            let progressed = match cell.try_borrow_mut() {
                Ok(mut ctx) => self.run_promise_job(&mut ctx).is_some(),
                Err(_) => false,
            };
            if !progressed {
                return Err(JsNativeError::typ()
                    .with_message("an asynchronous job could not complete")
                    .into());
            }
        }
    }
}

impl Executor {
    fn run_microtask(&self, cell: &RefCell<&mut Context>) -> Option<JsResult<()>> {
        let mut ctx = cell.try_borrow_mut().ok()?;
        if let Some(Ran::Job(_, result)) = self.run_promise_job(&mut ctx) {
            return Some(result);
        }
        let generic = self.generic_jobs.borrow_mut().pop_front();
        generic.map(|job| job.call(&mut ctx).map(drop))
    }

    fn run_due_timeout(&self, cell: &RefCell<&mut Context>) -> Option<JsResult<()>> {
        let mut ctx = cell.try_borrow_mut().ok()?;
        let now = Instant::now();
        let job = {
            let mut deadlines = self.deadlines.borrow_mut();
            let index = deadlines
                .iter()
                .enumerate()
                .filter(|(_, (deadline, _))| *deadline <= now)
                .min_by_key(|(_, (deadline, _))| *deadline)
                .map(|(index, _)| index)?;
            deadlines.remove(index).1
        };
        if job.cancelled() {
            return Some(Ok(()));
        }
        Some(job.call(&mut ctx).map(drop))
    }

    fn run_jobs_to_completion(&self, context: &mut Context) -> JsResult<()> {
        use core::future::Future;
        use core::task::{Context as TaskContext, Poll, Waker};
        let cell = RefCell::new(context);
        let mut futures = Vec::new();
        let mut task = TaskContext::from_waker(Waker::noop());
        loop {
            let mut progressed = false;
            while let Some(result) = self.run_microtask(&cell) {
                result?;
                progressed = true;
            }
            loop {
                let job = self.async_jobs.borrow_mut().pop_front();
                let Some(job) = job else {
                    break;
                };
                futures.push(job.call(&cell));
                progressed = true;
            }
            let mut failure = None;
            futures.retain_mut(|future| match core::pin::Pin::new(future).poll(&mut task) {
                Poll::Pending => true,
                Poll::Ready(result) => {
                    progressed = true;
                    if let Err(error) = result {
                        failure.get_or_insert(error);
                    }
                    false
                }
            });
            if let Some(error) = failure {
                return Err(error);
            }
            if progressed {
                continue;
            }
            if let Some(result) = self.run_due_timeout(&cell) {
                result?;
                continue;
            }
            let pending_timeouts = self.deadlines.borrow().len();
            if futures.is_empty() && pending_timeouts == 0 {
                break;
            }
            let pause = self
                .deadlines
                .borrow()
                .iter()
                .map(|(deadline, _)| deadline.saturating_duration_since(Instant::now()))
                .min()
                .unwrap_or(Duration::from_millis(1))
                .min(Duration::from_millis(1));
            std::thread::sleep(pause);
        }
        if let Ok(mut ctx) = cell.try_borrow_mut() {
            ctx.clear_kept_objects();
        }
        Ok(())
    }
}

impl JobExecutor for Executor {
    fn enqueue_job(self: Rc<Self>, job: Job, _context: &mut Context) {
        match job {
            Job::PromiseJob(job) => self.promise_jobs.borrow_mut().push_back(job),
            Job::GenericJob(job) => self.generic_jobs.borrow_mut().push_back(job),
            Job::AsyncJob(job) => self.async_jobs.borrow_mut().push_back(job),
            Job::TimeoutJob(job) if self.wait => {
                let deadline = Instant::now() + Duration::from_millis(job.timeout().as_millis());
                self.deadlines.borrow_mut().push((deadline, job));
            }
            Job::TimeoutJob(job) => self.timeout_jobs.borrow_mut().push_back(job),
            _ => {}
        }
    }

    fn run_jobs(self: Rc<Self>, context: &mut Context) -> JsResult<()> {
        if self.wait {
            return self.run_jobs_to_completion(context);
        }
        loop {
            match self.run_one(context) {
                Ran::Nothing => break,
                Ran::Job(_, Err(error)) => return Err(error),
                Ran::Job(_, Ok(())) => {}
            }
        }
        context.clear_kept_objects();
        Ok(())
    }
}

struct Hooks {
    runtime: RefCell<Weak<BoaRuntime>>,
}

impl Hooks {
    fn runtime(&self) -> Option<Rc<BoaRuntime>> {
        self.runtime.borrow().upgrade()
    }
}

impl HostHooks for Hooks {
    fn promise_rejection_tracker(
        &self,
        promise: &JsObject<boa_engine::builtins::promise::Promise>,
        operation: OperationType,
        context: &mut Context,
    ) {
        let Some(runtime) = self.runtime() else {
            return;
        };
        let Some(tracker) = runtime.options.rejection_tracker else {
            return;
        };
        let promise: JsObject = promise.clone().upcast();
        let reason = JsPromise::from_object(promise.clone())
            .ok()
            .and_then(|promise| match promise.state() {
                boa_engine::builtins::promise::PromiseState::Rejected(reason) => Some(reason),
                _ => None,
            })
            .unwrap_or_default();
        let handled = matches!(operation, OperationType::Handle);
        let _host = super::access::enter(context);
        tracker(
            &mut Scope::of(context),
            &Value(promise.into()),
            &Value(reason),
            handled,
        );
    }

    fn ensure_can_compile_strings(
        &self,
        realm: Realm,
        _parameters: &[JsString],
        _body: &JsString,
        _direct: bool,
        context: &mut Context,
    ) -> JsResult<()> {
        let Some(runtime) = self.runtime() else {
            return Ok(());
        };
        let Some(check) = runtime.options.compile_strings else {
            return Ok(());
        };
        let caller = context.script_realm().clone();
        let caller = handle_of_realm(context, &caller);
        let callee = handle_of_realm(context, &realm);
        if check(&mut Scope::of(context), caller, callee) {
            return Ok(());
        }
        Err(JsNativeError::eval()
            .with_message("Refused to evaluate a string as JavaScript")
            .into())
    }

    fn has_source_text_available(&self, function: &JsFunction, _context: &mut Context) -> bool {
        !super::access::is_engine_function(function)
    }
}

struct Loader {
    simple: Option<Rc<SimpleModuleLoader>>,
    runtime: RefCell<Weak<BoaRuntime>>,
    modules: RefCell<HashMap<String, Vec<(Realm, Module)>>>,
}

impl Loader {
    fn hooked(
        &self,
        referrer: &Referrer,
        request: &ModuleRequest,
        context: &mut Context,
    ) -> JsResult<Module> {
        let runtime = self.runtime.borrow().upgrade();
        let options = runtime.map(|runtime| runtime.options).unwrap_or_default();
        let Some(load) = options.module_load else {
            return Err(JsNativeError::typ()
                .with_message("module loading is not available")
                .into());
        };
        let base = referrer
            .path()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let specifier = request.specifier().to_std_string_escaped();
        let _host = super::access::enter(context);
        let mut scope = Scope::of(context);
        let name = match options.module_normalize {
            Some(normalize) => normalize(&mut scope, &base, &specifier),
            None => Ok(specifier),
        }
        .map_err(|error| JsError::from_opaque(error.0))?;
        let realm = scope.cx().realm().clone();
        if let Some(module) = self.loaded(&name, &realm) {
            return Ok(module);
        }
        let attributes = scope.new_object();
        for attribute in request.attributes() {
            let key = attribute.key().to_std_string_escaped();
            let value = Value(JsValue::from(attribute.value().clone()));
            scope
                .set(&attributes, &key, value)
                .map_err(|error| JsError::from_opaque(error.0))?;
        }
        let compiled =
            load(&mut scope, &name, &attributes).map_err(|error| JsError::from_opaque(error.0))?;
        let module = compiled.0.as_object().and_then(|object| {
            match &*object.downcast_ref::<Compiled>()? {
                Compiled::Module(module) => Some(module.clone()),
                Compiled::Script(_) => None,
            }
        });
        let module = module.ok_or_else(|| {
            JsError::from(
                JsNativeError::typ().with_message("module loader did not return a module"),
            )
        })?;
        self.modules
            .borrow_mut()
            .entry(name)
            .or_default()
            .push((realm, module.clone()));
        Ok(module)
    }

    fn forget_realm(&self, realm: &Realm) {
        self.modules.borrow_mut().retain(|_, modules| {
            modules.retain(|(owner, _)| owner != realm);
            !modules.is_empty()
        });
    }

    fn loaded(&self, name: &str, realm: &Realm) -> Option<Module> {
        self.modules
            .borrow()
            .get(name)?
            .iter()
            .find(|(owner, _)| owner == realm)
            .map(|(_, module)| module.clone())
    }
}

impl ModuleLoader for Loader {
    async fn load_imported_module(
        self: Rc<Self>,
        referrer: Referrer,
        request: ModuleRequest,
        context: &RefCell<&mut Context>,
    ) -> JsResult<Module> {
        if let Some(simple) = self.simple.clone() {
            let specifier = request.specifier().to_std_string_escaped();
            let defined = self.loaded(&specifier, context.borrow().realm());
            if let Some(module) = defined {
                return Ok(module);
            }
            return simple
                .load_imported_module(referrer, request, context)
                .await;
        }
        self.hooked(&referrer, &request, &mut context.borrow_mut())
    }

    fn get_module_source_module_record(
        self: Rc<Self>,
        source: &JsObject,
        _context: &mut Context,
    ) -> Option<Module> {
        self.modules
            .borrow()
            .values()
            .flatten()
            .map(|(_, module)| module)
            .find(|module| {
                module
                    .module_source()
                    .is_some_and(|object| JsObject::equals(&object, source))
            })
            .cloned()
    }
}

impl BoaRuntime {
    fn build(options: &RuntimeOptions, simple: Option<SimpleModuleLoader>) -> RuntimeHandle {
        let executor = Rc::new(Executor {
            wait: options.wait_for_jobs,
            ..Executor::default()
        });
        let hooks = Rc::new(Hooks {
            runtime: RefCell::new(Weak::new()),
        });
        let loader = Rc::new(Loader {
            simple: simple.map(Rc::new),
            runtime: RefCell::new(Weak::new()),
            modules: RefCell::new(HashMap::new()),
        });
        let Ok(mut context) = Context::builder()
            .host_hooks(hooks.clone())
            .job_executor(executor.clone())
            .module_loader(loader.clone())
            .can_block(options.can_block)
            .build()
        else {
            return RuntimeHandle::NULL;
        };
        if let Some(interrupt) = options.interrupt {
            let opaque = options.interrupt_opaque;
            let handler: Rc<dyn Fn() -> bool> = Rc::new(move || interrupt(opaque));
            context.set_interrupt_handler(Some(handler));
        }
        if options.stack_size != 0 {
            context.set_native_stack_limit(options.stack_size);
            let limits = context.runtime_limits_mut();
            limits.set_recursion_limit(CALL_DEPTH_LIMIT);
            limits.set_stack_size_limit(VALUE_STACK_LIMIT);
        }
        let runtime = Rc::new(BoaRuntime {
            ctx: UnsafeCell::new(context),
            options: *options,
            opaque: Cell::new(ptr::null_mut()),
            executor,
            loader: loader.clone(),
            records: RefCell::new(Vec::new()),
        });
        *hooks.runtime.borrow_mut() = Rc::downgrade(&runtime);
        *loader.runtime.borrow_mut() = Rc::downgrade(&runtime);
        let handle = RuntimeHandle(Rc::as_ptr(&runtime).cast_mut().cast());
        let ctx = runtime.ctx.get() as usize;
        CONTEXTS.with(|contexts| contexts.borrow_mut().push((ctx, handle.0 as usize)));
        RUNTIMES.with(|runtimes| runtimes.borrow_mut().insert(handle.0 as usize, runtime));
        handle
    }

    fn with_context<R>(&self, f: impl FnOnce(&mut Context) -> R) -> R {
        // SAFETY: the Context lives in this runtime, which the caller holds through an Rc for
        // the whole call, so it outlives the reference. A runtime never leaves its thread (it
        // holds Rc and Cell), and the host enters it only here and from the natives and hooks
        // Boa calls with its own &mut Context, which Boa does not touch until they return, so
        // the reference handed to f is the only one in use while f runs.
        f(unsafe { &mut *self.ctx.get() })
    }

    fn record(self: &Rc<Self>, realm: Realm, opaque: *mut c_void) -> ContextHandle {
        let record = Rc::new(RealmRecord {
            runtime: Rc::downgrade(self),
            realm: realm.clone(),
            opaque: Cell::new(opaque),
        });
        let handle = ContextHandle(Rc::as_ptr(&record).cast_mut().cast());
        let key = handle.0 as usize;
        realm.host_defined_mut().insert(RealmTag(key));
        self.records.borrow_mut().push(key);
        RECORDS.with(|records| records.borrow_mut().insert(key, record));
        handle
    }
}

pub(super) fn engine_runtime(module_root: &Path, options: &RuntimeOptions) -> RuntimeHandle {
    BoaRuntime::build(options, SimpleModuleLoader::new(module_root).ok())
}

pub(super) fn enter_default<R>(rt: RuntimeHandle, f: impl FnOnce(&mut Context) -> R) -> Option<R> {
    rt.runtime().map(|runtime| runtime.with_context(f))
}

fn handle_of_realm(ctx: &Context, realm: &Realm) -> ContextHandle {
    if let Some(tag) = realm.host_defined().get::<RealmTag>() {
        return ContextHandle(tag.0 as *mut c_void);
    }
    let Some(runtime) = runtime_of(ctx) else {
        return ContextHandle::NULL;
    };
    runtime.record(realm.clone(), ptr::null_mut())
}

fn runtime_of(ctx: &Context) -> Option<Rc<BoaRuntime>> {
    let key = ptr::from_ref(ctx) as usize;
    let runtime = CONTEXTS.with(|contexts| {
        contexts
            .borrow()
            .iter()
            .find(|(c, _)| *c == key)
            .map(|(_, r)| *r)
    })?;
    registered(&RUNTIMES, runtime)
}

impl RuntimeHandle {
    fn runtime(self) -> Option<Rc<BoaRuntime>> {
        registered(&RUNTIMES, self.0 as usize)
    }

    pub fn new(options: &RuntimeOptions) -> RuntimeHandle {
        BoaRuntime::build(options, None)
    }

    pub fn free(self) {
        let Some(runtime) = unregister(&RUNTIMES, self.0 as usize) else {
            return;
        };
        let ctx = runtime.ctx.get() as usize;
        CONTEXTS.with(|contexts| contexts.borrow_mut().retain(|(c, _)| *c != ctx));
        for record in runtime.records.take() {
            drop(unregister(&RECORDS, record));
        }
        drop(runtime);
    }

    pub fn opaque(self) -> *mut c_void {
        self.runtime()
            .map_or(ptr::null_mut(), |runtime| runtime.opaque.get())
    }

    pub fn set_opaque(self, opaque: *mut c_void) {
        if let Some(runtime) = self.runtime() {
            runtime.opaque.set(opaque);
        }
    }

    pub fn set_max_stack_size(self, _bytes: usize) {}

    pub fn new_context(self, opaque: *mut c_void) -> ContextHandle {
        let Some(runtime) = self.runtime() else {
            return ContextHandle::NULL;
        };
        match runtime.with_context(|ctx| ctx.create_realm()) {
            Ok(realm) => runtime.record(realm, opaque),
            Err(_) => ContextHandle::NULL,
        }
    }

    pub fn has_pending_jobs(self) -> bool {
        self.runtime()
            .is_some_and(|runtime| !runtime.executor.is_empty())
    }

    pub fn pending_job_context(self) -> ContextHandle {
        let Some(runtime) = self.runtime() else {
            return ContextHandle::NULL;
        };
        runtime.with_context(|ctx| match runtime.executor.front_realm() {
            Some(realm) => handle_of_realm(ctx, &realm),
            None if runtime.executor.is_empty() => ContextHandle::NULL,
            None => {
                let realm = ctx.realm().clone();
                handle_of_realm(ctx, &realm)
            }
        })
    }

    pub fn run_pending_job(self) -> JobOutcome {
        let Some(runtime) = self.runtime() else {
            return JobOutcome::Idle;
        };
        runtime.with_context(|ctx| match runtime.executor.run_one(ctx) {
            Ran::Nothing => JobOutcome::Idle,
            Ran::Job(realm, result) => {
                let realm = realm.unwrap_or_else(|| ctx.realm().clone());
                let handle = handle_of_realm(ctx, &realm);
                match result {
                    Ok(()) => JobOutcome::Ran(handle),
                    Err(error) => {
                        let mut scope = Scope::of(ctx);
                        JobOutcome::Failed(handle, scope.error(error))
                    }
                }
            }
        })
    }

    pub fn gc(self) {
        if !self.is_null() {
            boa_gc::force_collect();
        }
    }

    pub fn memory_usage(self) -> MemoryUsage {
        MemoryUsage::default()
    }
}

impl ContextHandle {
    fn record(self) -> Option<Rc<RealmRecord>> {
        registered(&RECORDS, self.0 as usize)
    }

    pub fn with<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        let record = self.record()?;
        let runtime = record.runtime.upgrade()?;
        let result = runtime.with_context(|ctx| {
            let current = ctx.realm().clone();
            let _host = super::access::enter(ctx);
            if current == record.realm {
                return f(&mut Scope::of(ctx));
            }
            let previous = ctx.enter_realm(record.realm.clone());
            let result = f(&mut Scope::of(ctx));
            ctx.enter_realm(previous);
            result
        });
        Some(result)
    }

    pub fn opaque(self) -> *mut c_void {
        self.record()
            .map_or(ptr::null_mut(), |record| record.opaque.get())
    }

    pub fn set_opaque(self, opaque: *mut c_void) {
        if let Some(record) = self.record() {
            record.opaque.set(opaque);
        }
    }

    pub fn runtime(self) -> RuntimeHandle {
        self.record().map_or(RuntimeHandle::NULL, |record| {
            RuntimeHandle(record.runtime.as_ptr().cast_mut().cast())
        })
    }

    pub fn free(self) {
        let Some(record) = self.record() else {
            return;
        };
        let key = self.0 as usize;
        record.realm.host_defined_mut().remove::<RealmTag>();
        if let Some(runtime) = record.runtime.upgrade() {
            runtime.records.borrow_mut().retain(|&r| r != key);
            runtime.loader.forget_realm(&record.realm);
        }
        drop(unregister(&RECORDS, key));
    }
}

impl Scope<'_> {
    pub fn handle(&self) -> ContextHandle {
        let realm = self.ctx.realm().clone();
        handle_of_realm(self.ctx, &realm)
    }

    pub fn runtime(&self) -> RuntimeHandle {
        runtime_of(self.ctx).map_or(RuntimeHandle::NULL, |runtime| {
            RuntimeHandle(Rc::as_ptr(&runtime).cast_mut().cast())
        })
    }

    pub(super) fn register_module(&mut self, path: &Path, module: &Module) {
        let Some(runtime) = runtime_of(self.cx()) else {
            return;
        };
        if let Some(simple) = &runtime.loader.simple {
            simple.insert(path.to_path_buf(), module.clone());
        }
    }

    pub fn mapped_module(&mut self, specifier: &str) -> Option<Value> {
        let runtime = runtime_of(self.cx())?;
        let realm = self.cx().realm().clone();
        let module = runtime.loader.loaded(specifier, &realm)?;
        Some(self.compiled(Compiled::Module(module)))
    }

    pub fn map_module(&mut self, specifier: &str, module: &Value) {
        let Some(module) =
            module
                .0
                .as_object()
                .and_then(|object| match &*object.downcast_ref::<Compiled>()? {
                    Compiled::Module(module) => Some(module.clone()),
                    Compiled::Script(_) => None,
                })
        else {
            return;
        };
        let Some(runtime) = runtime_of(self.cx()) else {
            return;
        };
        let realm = self.cx().realm().clone();
        if runtime.loader.loaded(specifier, &realm).is_some() {
            return;
        }
        runtime
            .loader
            .modules
            .borrow_mut()
            .entry(specifier.to_owned())
            .or_default()
            .push((realm, module));
    }

    pub fn define_module_source(
        &mut self,
        specifier: &str,
        source: &Value,
        kind: &str,
    ) -> Result<(), Value> {
        let Some(object) = source.0.as_object() else {
            return Err(self.type_error("a module source must be an object"));
        };
        let Some(runtime) = runtime_of(self.cx()) else {
            return Err(self.type_error("module loading is not available"));
        };
        let module = Module::synthetic(
            &[],
            SyntheticModuleInitializer::from_copy_closure(|_, _| Ok(())),
            None,
            None,
            self.cx(),
        );
        module.set_module_source(object, JsString::from(kind));
        let realm = self.cx().realm().clone();
        runtime
            .loader
            .modules
            .borrow_mut()
            .entry(specifier.to_owned())
            .or_default()
            .push((realm, module));
        Ok(())
    }

    pub fn context_opaque(&self) -> *mut c_void {
        self.handle().opaque()
    }

    pub fn caller_handle(&mut self) -> ContextHandle {
        self.handle()
    }

    pub fn calling_frame_handle(&mut self) -> ContextHandle {
        match self.cx().native_caller_realm().cloned() {
            Some(realm) => handle_of_realm(self.cx(), &realm),
            None => self.handle(),
        }
    }

    pub fn function_handle(&mut self, function: &Value) -> Result<ContextHandle, Value> {
        let Some(object) = function.0.as_object() else {
            return Err(self.type_error("not a function"));
        };
        let realm = object
            .get_function_realm(self.cx())
            .map_err(|error| self.error(error))?;
        Ok(handle_of_realm(self.cx(), &realm))
    }

    pub fn is_running_script(&mut self) -> bool {
        self.cx().frame_depth() > 1
    }
}
