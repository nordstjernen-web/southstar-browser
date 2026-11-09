/* Southstar — where the renderer keeps its per-user files.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

package org.southstar.internal;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * The per-user directories the renderer resolves ({@code rust/glib}'s
 * {@code dirs.rs}), worked out again on the Java side.
 */
public final class EngineDirs {

    private EngineDirs() {
    }

    /**
     * The directories the renderer's user cache directory may be for this
     * user: {@code XDG_CACHE_HOME} when set (honoured on every platform),
     * then the platform default: {@code ~/.cache} on Unix and macOS, the
     * Internet cache folder under the local application data directory on
     * Windows.
     */
    public static List<Path> userCacheDirs() {
        List<Path> dirs = new ArrayList<>();
        String xdg = System.getenv("XDG_CACHE_HOME");
        if (xdg != null && !xdg.isEmpty()) {
            dirs.add(Path.of(xdg));
        }
        String os = System.getProperty("os.name", "").toLowerCase(Locale.ROOT);
        String home = System.getProperty("user.home", "");
        if (os.startsWith("windows")) {
            String local = System.getenv("LOCALAPPDATA");
            Path base = local != null && !local.isEmpty()
                ? Path.of(local) : Path.of(home, "AppData", "Local");
            dirs.add(base.resolve("Microsoft").resolve("Windows").resolve("INetCache"));
            dirs.add(Path.of(home, ".cache"));
        } else {
            dirs.add(Path.of(home, ".cache"));
        }
        return dirs;
    }

    /** {@code <cache>/southstar/<name>} for every candidate cache directory. */
    public static List<Path> cacheSubdirs(String name) {
        List<Path> dirs = new ArrayList<>();
        for (Path cache : userCacheDirs()) {
            dirs.add(cache.resolve("southstar").resolve(name));
        }
        return dirs;
    }
}
