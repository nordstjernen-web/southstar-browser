/* Southstar — PageExample: opens one page through the browser API, prints what it found and saves a screenshot.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

import org.southstar.browser.BrowserContext;
import org.southstar.browser.Engine;
import org.southstar.browser.Page;
import org.southstar.browser.Response;

import java.nio.file.Files;
import java.nio.file.Path;

public final class PageExample {

    public static void main(String[] args) throws Exception {
        String url = args.length > 0 ? args[0] : "about:start";
        try (Engine engine = Engine.launch();
             BrowserContext context = engine.newContext();
             Page page = context.newPage(1000, 700)) {
            Response response = page.navigate(url);
            System.out.println("ok:     " + response.ok());
            System.out.println("engine: " + page.engineVersion());
            System.out.println("title:  " + page.title());
            System.out.println("url:    " + page.url());
            System.out.println("size:   " + page.pageWidth() + "x" + page.pageHeight() + " css px");

            String text = page.text().replace('\n', ' ').strip();
            System.out.println("text:   " + text.substring(0, Math.min(160, text.length())));
            System.out.println("links:  " + page.links().size() + " found");
            System.out.println("script: 6 * 7 = " + page.evaluate("6 * 7"));

            byte[] png = page.screenshot();
            Path out = Path.of("page.png");
            Files.write(out, png);
            System.out.println("wrote:  " + out.toAbsolutePath() + " (" + png.length + " bytes)");
        }
    }
}
