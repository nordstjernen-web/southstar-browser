/* Southstar — a renderer request that passed its deadline.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

package org.southstar;

/**
 * Thrown when the renderer process did not answer a request within its
 * deadline. The renderer has been stopped by then; navigate the page again,
 * which starts a fresh one, before going on.
 */
public class RendererTimeoutException extends SouthstarException {

    private static final long serialVersionUID = 1L;

    public RendererTimeoutException(String message, Throwable cause) {
        super(message, cause);
    }
}
