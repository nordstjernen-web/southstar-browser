//! Southstar — runtimes, contexts and their handles on Boa: a runtime owns one Context, a context handle names one of its realms, and a scope re-enters that realm on demand.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::c_void;
use core::ptr;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::rc::Rc;

use boa_engine::builtins::promise::OperationType;
use boa_engine::context::HostHooks;
use boa_engine::job::{GenericJob, Job, JobExecutor, NativeAsyncJob, PromiseJob, TimeoutJob};
use boa_engine::module::{ModuleLoader, ModuleRequest, Referrer, SimpleModuleLoader};
use boa_engine::object::builtins::JsPromise;
use boa_engine::prelude::{Finalize, JsData, Trace};
use boa_engine::realm::Realm;
use boa_engine::{Context, JsError, JsNativeError, JsObject, JsResult, JsValue, Module};

use super::{Compiled, Scope, Value};
use crate::{ContextHandle, JobOutcome, MemoryUsage, RuntimeHandle, RuntimeOptions};

pub(super) struct BoaRuntime {
    ctx: *mut Context,
    options: RuntimeOptions,
    opaque: Cell<*mut c_void>,
    executor: Rc<Executor>,
    loader: Rc<Loader>,
    records: RefCell<Vec<*mut RealmRecord>>,
}

pub(super) struct RealmRecord {
    runtime: *mut BoaRuntime,
    realm: Realm,
    opaque: Cell<*mut c_void>,
}

#[derive(Trace, Finalize, JsData)]
struct RealmTag(usize);

thread_local! {
    static LIVE: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    static CONTEXTS: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
}

fn live(record: *const RealmRecord) -> bool {
    LIVE.with(|live| live.borrow().contains(&(record as usize)))
}

#[derive(Default)]
struct Executor {
    promise_jobs: RefCell<VecDeque<PromiseJob>>,
    generic_jobs: RefCell<VecDeque<GenericJob>>,
    async_jobs: RefCell<VecDeque<NativeAsyncJob>>,
    timeout_jobs: RefCell<VecDeque<TimeoutJob>>,
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

impl JobExecutor for Executor {
    fn enqueue_job(self: Rc<Self>, job: Job, _context: &mut Context) {
        match job {
            Job::PromiseJob(job) => self.promise_jobs.borrow_mut().push_back(job),
            Job::GenericJob(job) => self.generic_jobs.borrow_mut().push_back(job),
            Job::AsyncJob(job) => self.async_jobs.borrow_mut().push_back(job),
            Job::TimeoutJob(job) => self.timeout_jobs.borrow_mut().push_back(job),
            _ => {}
        }
    }

    fn run_jobs(self: Rc<Self>, context: &mut Context) -> JsResult<()> {
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
    runtime: Cell<*const BoaRuntime>,
}

impl HostHooks for Hooks {
    fn promise_rejection_tracker(
        &self,
        promise: &JsObject<boa_engine::builtins::promise::Promise>,
        operation: OperationType,
        context: &mut Context,
    ) {
        let Some(runtime) = (unsafe { self.runtime.get().as_ref() }) else {
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
        tracker(
            &mut Scope::of(context),
            &Value(promise.into()),
            &Value(reason),
            handled,
        );
    }
}

struct Loader {
    simple: Option<Rc<SimpleModuleLoader>>,
    runtime: Cell<*const BoaRuntime>,
    modules: RefCell<HashMap<String, Vec<(Realm, Module)>>>,
}

impl Loader {
    fn hooked(
        &self,
        referrer: &Referrer,
        request: &ModuleRequest,
        context: &mut Context,
    ) -> JsResult<Module> {
        let runtime = unsafe { self.runtime.get().as_ref() };
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
            return simple
                .load_imported_module(referrer, request, context)
                .await;
        }
        self.hooked(&referrer, &request, &mut context.borrow_mut())
    }
}

impl BoaRuntime {
    fn build(options: &RuntimeOptions, simple: Option<SimpleModuleLoader>) -> RuntimeHandle {
        let executor = Rc::new(Executor::default());
        let hooks = Rc::new(Hooks {
            runtime: Cell::new(ptr::null()),
        });
        let loader = Rc::new(Loader {
            simple: simple.map(Rc::new),
            runtime: Cell::new(ptr::null()),
            modules: RefCell::new(HashMap::new()),
        });
        let Ok(mut context) = Context::builder()
            .host_hooks(hooks.clone())
            .job_executor(executor.clone())
            .module_loader(loader.clone())
            .build()
        else {
            return RuntimeHandle::NULL;
        };
        if let Some(interrupt) = options.interrupt {
            let opaque = options.interrupt_opaque;
            let handler: Rc<dyn Fn() -> bool> = Rc::new(move || interrupt(opaque));
            context.set_interrupt_handler(Some(handler));
        }
        let runtime = Box::into_raw(Box::new(BoaRuntime {
            ctx: Box::into_raw(Box::new(context)),
            options: *options,
            opaque: Cell::new(ptr::null_mut()),
            executor,
            loader: loader.clone(),
            records: RefCell::new(Vec::new()),
        }));
        hooks.runtime.set(runtime);
        loader.runtime.set(runtime);
        let ctx = unsafe { (*runtime).ctx } as usize;
        CONTEXTS.with(|contexts| contexts.borrow_mut().push((ctx, runtime as usize)));
        RuntimeHandle(runtime.cast())
    }

    fn record(&self, realm: Realm, opaque: *mut c_void) -> ContextHandle {
        let record = Box::into_raw(Box::new(RealmRecord {
            runtime: ptr::from_ref(self).cast_mut(),
            realm: realm.clone(),
            opaque: Cell::new(opaque),
        }));
        realm.host_defined_mut().insert(RealmTag(record as usize));
        self.records.borrow_mut().push(record);
        LIVE.with(|live| live.borrow_mut().insert(record as usize));
        ContextHandle(record.cast())
    }
}

pub(super) fn engine_runtime(module_root: &Path) -> RuntimeHandle {
    BoaRuntime::build(
        &RuntimeOptions::default(),
        SimpleModuleLoader::new(module_root).ok(),
    )
}

pub(super) fn default_context(rt: RuntimeHandle) -> *mut Context {
    rt.runtime().map_or(ptr::null_mut(), |runtime| runtime.ctx)
}

fn handle_of_realm(ctx: &mut Context, realm: &Realm) -> ContextHandle {
    if let Some(tag) = realm.host_defined().get::<RealmTag>() {
        return ContextHandle(tag.0 as *mut c_void);
    }
    let Some(runtime) = runtime_of(ctx) else {
        return ContextHandle::NULL;
    };
    runtime.record(realm.clone(), ptr::null_mut())
}

fn runtime_of<'a>(ctx: &Context) -> Option<&'a BoaRuntime> {
    let key = ptr::from_ref(ctx) as usize;
    let runtime = CONTEXTS.with(|contexts| {
        contexts
            .borrow()
            .iter()
            .find(|(c, _)| *c == key)
            .map(|(_, r)| *r)
    })?;
    unsafe { (runtime as *const BoaRuntime).as_ref() }
}

impl RuntimeHandle {
    fn runtime<'a>(self) -> Option<&'a BoaRuntime> {
        unsafe { self.0.cast::<BoaRuntime>().as_ref() }
    }

    pub fn new(options: &RuntimeOptions) -> RuntimeHandle {
        BoaRuntime::build(options, None)
    }

    pub unsafe fn free(self) {
        let raw = self.0.cast::<BoaRuntime>();
        if raw.is_null() {
            return;
        }
        let runtime = unsafe { Box::from_raw(raw) };
        let ctx = runtime.ctx as usize;
        CONTEXTS.with(|contexts| contexts.borrow_mut().retain(|(c, _)| *c != ctx));
        for record in runtime.records.take() {
            LIVE.with(|live| live.borrow_mut().remove(&(record as usize)));
            drop(unsafe { Box::from_raw(record) });
        }
        drop(unsafe { Box::from_raw(runtime.ctx) });
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
        let ctx = unsafe { &mut *runtime.ctx };
        match ctx.create_realm() {
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
        let ctx = unsafe { &mut *runtime.ctx };
        match runtime.executor.front_realm() {
            Some(realm) => handle_of_realm(ctx, &realm),
            None if runtime.executor.is_empty() => ContextHandle::NULL,
            None => {
                let realm = ctx.realm().clone();
                handle_of_realm(ctx, &realm)
            }
        }
    }

    pub fn run_pending_job(self) -> JobOutcome {
        let Some(runtime) = self.runtime() else {
            return JobOutcome::Idle;
        };
        let ctx = unsafe { &mut *runtime.ctx };
        match runtime.executor.run_one(ctx) {
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
        }
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
    fn record<'a>(self) -> Option<&'a RealmRecord> {
        let record = self.0.cast::<RealmRecord>();
        if record.is_null() || !live(record) {
            return None;
        }
        unsafe { record.as_ref() }
    }

    pub fn with<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        let record = self.record()?;
        let ctx = unsafe { (*record.runtime).ctx };
        let current = unsafe { (*ctx).realm().clone() };
        if current == record.realm {
            return Some(f(&mut Scope::of(ctx)));
        }
        let previous = unsafe { (*ctx).enter_realm(record.realm.clone()) };
        let result = f(&mut Scope::of(ctx));
        unsafe { (*ctx).enter_realm(previous) };
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
            RuntimeHandle(record.runtime.cast())
        })
    }

    pub unsafe fn free(self) {
        let Some(record) = self.record() else {
            return;
        };
        let raw = ptr::from_ref(record).cast_mut();
        record.realm.host_defined_mut().remove::<RealmTag>();
        if let Some(runtime) = unsafe { record.runtime.as_ref() } {
            runtime.records.borrow_mut().retain(|&r| r != raw);
            runtime.loader.forget_realm(&record.realm);
        }
        LIVE.with(|live| live.borrow_mut().remove(&(raw as usize)));
        drop(unsafe { Box::from_raw(raw) });
    }
}

impl Scope<'_> {
    pub fn handle(&self) -> ContextHandle {
        let ctx = unsafe { &mut *self.ctx };
        let realm = ctx.realm().clone();
        handle_of_realm(ctx, &realm)
    }

    pub fn runtime(&self) -> RuntimeHandle {
        let ctx = unsafe { &*self.ctx };
        runtime_of(ctx).map_or(RuntimeHandle::NULL, |runtime| {
            RuntimeHandle(ptr::from_ref(runtime).cast_mut().cast())
        })
    }

    pub fn context_opaque(&self) -> *mut c_void {
        self.handle().opaque()
    }

    pub fn caller_handle(&mut self) -> ContextHandle {
        self.handle()
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
        false
    }
}
