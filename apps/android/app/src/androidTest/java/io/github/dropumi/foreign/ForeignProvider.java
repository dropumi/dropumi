// SPDX-License-Identifier: MPL-2.0
//
// A provider of another app, for uriGrantSurvivesShareActivityFinish. It is
// declared in the test APK's manifest, so it runs in the test package's own
// process (":foreign") under the test package's UID, never the app's: the
// app can read its one document only through a URI grant. It is not
// exported, and it grants per URI. ForeignSharer (same package, same UID)
// shares the document to Dropumi's share target with a read grant.
//
// Java, not Kotlin: the test APK leaves out what the app APK already has
// (the Kotlin standard library among it), and this class runs outside the
// app's process, where only the test APK's own classes exist.

package io.github.dropumi.foreign;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.content.Context;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import java.io.File;
import java.io.FileNotFoundException;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;

public final class ForeignProvider extends ContentProvider {
    private static final String FILE_NAME = "foreign-document.txt";
    private static final String DOCUMENT_PATH = "/document";

    /** The document's name, as the provider reports it. */
    public static final String DISPLAY_NAME = "notes.txt";

    /** The document's bytes. */
    public static byte[] content() {
        StringBuilder text = new StringBuilder();
        for (int i = 0; i < 100; i++) {
            text.append("shared by another app\n");
        }
        return text.toString().getBytes(StandardCharsets.UTF_8);
    }

    /** The one document, in the test package [testContext] belongs to. */
    public static Uri documentUri(Context testContext) {
        return Uri.parse("content://" + testContext.getPackageName() + ".foreign" + DOCUMENT_PATH);
    }

    @Override
    public boolean onCreate() {
        return true;
    }

    private File document() throws FileNotFoundException {
        File file = new File(getContext().getCacheDir(), FILE_NAME);
        byte[] bytes = content();
        if (!file.isFile() || file.length() != bytes.length) {
            try (FileOutputStream out = new FileOutputStream(file)) {
                out.write(bytes);
            } catch (IOException e) {
                throw new FileNotFoundException("the document could not be written");
            }
        }
        return file;
    }

    @Override
    public Cursor query(Uri uri, String[] projection, String selection, String[] args, String sort) {
        if (!DOCUMENT_PATH.equals(uri.getPath())) {
            return null;
        }
        MatrixCursor cursor = new MatrixCursor(new String[] {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE});
        cursor.addRow(new Object[] {DISPLAY_NAME, (long) content().length});
        return cursor;
    }

    @Override
    public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        if (!DOCUMENT_PATH.equals(uri.getPath()) || !"r".equals(mode)) {
            throw new FileNotFoundException("no such document");
        }
        return ParcelFileDescriptor.open(document(), ParcelFileDescriptor.MODE_READ_ONLY);
    }

    @Override
    public String getType(Uri uri) {
        return "text/plain";
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        return null;
    }

    @Override
    public int delete(Uri uri, String selection, String[] args) {
        return 0;
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] args) {
        return 0;
    }
}
