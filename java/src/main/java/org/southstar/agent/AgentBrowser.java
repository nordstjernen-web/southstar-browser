/* Southstar — a browser for a program or a model to drive: tabs without a window.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

package org.southstar.agent;

import org.southstar.browser.BrowserContext;
import org.southstar.browser.ContextOptions;
import org.southstar.browser.Engine;
import org.southstar.browser.Page;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * A headless browser made for agents: tabs that keep their own scroll
 * position and history, follow the navigations a page asks for, and describe
 * the page as an outline whose elements carry references to act on.
 *
 * <pre>{@code
 * try (AgentBrowser browser = AgentBrowser.launch()) {
 *     Tab tab = browser.newTab("https://example.com");
 *     String outline = tab.snapshot(false, 40_000);   // ... link "More information..." [ref_3] ...
 *     tab.click("ref_3");
 *     byte[] png = tab.screenshot();
 * }
 * }</pre>
 *
 * <p>One action runs at a time across the whole browser; every method may
 * be called from any thread. {@link BrowserTools} offers the same actions as
 * named tools with JSON schemas, and {@link McpServer} serves those over the
 * Model Context Protocol.
 */
public final class AgentBrowser implements AutoCloseable {

    /**
     * How an agent's browser is set up.
     *
     * @param engine         how the engine is found and how long a request may take
     * @param context        the profile, resource policy and identity of the tabs
     * @param viewportWidth  the tabs' viewport width in CSS pixels
     * @param viewportHeight the tabs' viewport height in CSS pixels
     * @param downloads      where files a page offers are saved, or null to decline them
     */
    public record Options(Engine.Options engine, ContextOptions context, int viewportWidth,
                          int viewportHeight, Path downloads) {

        public Options {
            if (engine == null || context == null) {
                throw new IllegalArgumentException("engine and context must not be null");
            }
            if (viewportWidth < 200 || viewportHeight < 200) {
                throw new IllegalArgumentException("the viewport must be at least 200 pixels a side");
            }
        }

        /** A 1280 by 800 viewport, a profile deleted on close, no downloads. */
        public static Options defaults() {
            return new Options(Engine.Options.defaults(), ContextOptions.ephemeral(), 1280, 800,
                               null);
        }

        public Options withEngine(Engine.Options engine) {
            return new Options(engine, context, viewportWidth, viewportHeight, downloads);
        }

        public Options withContext(ContextOptions context) {
            return new Options(engine, context, viewportWidth, viewportHeight, downloads);
        }

        public Options withViewport(int width, int height) {
            return new Options(engine, context, width, height, downloads);
        }

        public Options withDownloads(Path directory) {
            return new Options(engine, context, viewportWidth, viewportHeight, directory);
        }
    }

    private final Options options;
    private final Engine engine;
    private final BrowserContext context;
    private final List<Tab> tabs = new ArrayList<>();
    private Tab active;
    private int nextTabId = 1;
    private boolean closed;

    private AgentBrowser(Options options) {
        this.options = options;
        this.engine = Engine.launch(options.engine());
        try {
            this.context = engine.newContext(options.context());
        } catch (RuntimeException e) {
            engine.close();
            throw e;
        }
    }

    public static AgentBrowser launch() {
        return new AgentBrowser(Options.defaults());
    }

    public static AgentBrowser launch(Options options) {
        return new AgentBrowser(options);
    }

    public Options options() {
        return options;
    }

    /** Open a tab, make it the active one and load {@code address} in it; null opens a blank tab. */
    public synchronized Tab newTab(String address) {
        Tab tab = addTab();
        active = tab;
        if (address != null && !address.isBlank()) {
            tab.navigate(address);
        }
        return tab;
    }

    /** The open tabs, oldest first. */
    public synchronized List<Tab> tabs() {
        return List.copyOf(tabs);
    }

    /** The tab actions go to when none is named, opening one if there is none. */
    public synchronized Tab activeTab() {
        if (active == null) {
            active = addTab();
        }
        return active;
    }

    /** The tab with this id; 0 or less means the active tab. */
    public synchronized Tab tab(int id) {
        if (id <= 0) {
            return activeTab();
        }
        for (Tab tab : tabs) {
            if (tab.id() == id) {
                return tab;
            }
        }
        throw new AgentException("there is no tab " + id + "; the open tabs are " + tabIds());
    }

    public synchronized void selectTab(int id) {
        active = tab(id);
    }

    /** Close a tab; the newest remaining one becomes active. */
    public synchronized void closeTab(int id) {
        Tab tab = tab(id);
        tabs.remove(tab);
        tab.page().close();
        if (active == tab) {
            active = tabs.isEmpty() ? null : tabs.get(tabs.size() - 1);
        }
    }

    /** One line per tab: id, URL and title, the active one marked. */
    public synchronized String describeTabs() {
        if (tabs.isEmpty()) {
            return "No tabs are open.";
        }
        StringBuilder out = new StringBuilder();
        for (Tab tab : tabs) {
            out.append(tab.describe()).append(tab == active ? " (active)" : "").append('\n');
        }
        return out.toString().stripTrailing();
    }

    @Override
    public synchronized void close() {
        if (closed) {
            return;
        }
        closed = true;
        tabs.clear();
        context.close();
        engine.close();
    }

    private Tab addTab() {
        if (closed) {
            throw new IllegalStateException("the browser is closed");
        }
        Page page = context.newPage(options.viewportWidth(), options.viewportHeight());
        Tab tab = new Tab(this, nextTabId++, page);
        tabs.add(tab);
        return tab;
    }

    private String tabIds() {
        List<Integer> ids = new ArrayList<>();
        for (Tab tab : tabs) {
            ids.add(tab.id());
        }
        return ids.toString();
    }
}
