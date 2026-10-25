// SPDX-License-Identifier: MPL-2.0
//
// The instrumentation runner. It always installs TestApplication, so the
// app's DropumiApplication (and with it the app's real configuration) is
// never built in an instrumented test (ADR-0013 D9).

package io.github.dropumi

import android.app.Application
import android.content.Context
import androidx.test.runner.AndroidJUnitRunner

class DropumiTestRunner : AndroidJUnitRunner() {
    override fun newApplication(cl: ClassLoader?, className: String?, context: Context?): Application =
        super.newApplication(cl, TestApplication::class.java.name, context)
}
