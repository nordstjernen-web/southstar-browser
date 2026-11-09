/* Southstar — the error the renderer and its client report.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

package org.southstar;

/** Thrown when the renderer is missing, fails a request or reports a failure. */
public class SouthstarException extends RuntimeException {

    private static final long serialVersionUID = 1L;

    public SouthstarException(String message) {
        super(message);
    }

    public SouthstarException(String message, Throwable cause) {
        super(message, cause);
    }
}
