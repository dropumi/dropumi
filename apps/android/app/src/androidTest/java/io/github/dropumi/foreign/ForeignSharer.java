// SPDX-License-Identifier: MPL-2.0
//
// Another app sharing a document to Dropumi: it runs in the test package's
// own process (":foreign") and UID, sends ForeignProvider's one document to
// Dropumi's ShareActivity with FLAG_GRANT_READ_URI_PERMISSION, and
// finishes. It is exported only so the test (in the app's process) can
// start it; it reads nothing from its intent, so it shares only that
// document. Java for the reason given in ForeignProvider.

package io.github.dropumi.foreign;

import android.app.Activity;
import android.content.ClipData;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;

public final class ForeignSharer extends Activity {
    private static final String TEST_SUFFIX = ".test";
    private static final String SHARE_ACTIVITY = "io.github.dropumi.ShareActivity";

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Uri uri = ForeignProvider.documentUri(this);
        String app = getPackageName().substring(0, getPackageName().length() - TEST_SUFFIX.length());
        Intent share = new Intent(Intent.ACTION_SEND)
            .setClassName(app, SHARE_ACTIVITY)
            .setType("text/plain")
            .putExtra(Intent.EXTRA_STREAM, uri)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        share.setClipData(ClipData.newRawUri("", uri));
        startActivity(share);
        finish();
    }
}
