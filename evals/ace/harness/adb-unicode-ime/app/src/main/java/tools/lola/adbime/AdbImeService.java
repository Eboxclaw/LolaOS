package tools.lola.adbime;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.inputmethodservice.InputMethodService;
import android.os.Build;
import android.view.inputmethod.InputConnection;

/** Minimal IME that commits broadcast-delivered text into the focused field. */
public class AdbImeService extends InputMethodService {

    public static final String ACTION_INPUT_TEXT = "tools.lola.adbime.INPUT_TEXT";
    public static final String EXTRA_TEXT = "text";

    private final BroadcastReceiver receiver = new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
            String text = intent.getStringExtra(EXTRA_TEXT);
            if (text == null) {
                return;
            }
            InputConnection connection = getCurrentInputConnection();
            if (connection != null) {
                connection.commitText(text, 1);
            }
        }
    };

    @Override
    public void onCreate() {
        super.onCreate();
        IntentFilter filter = new IntentFilter(ACTION_INPUT_TEXT);
        if (Build.VERSION.SDK_INT >= 33) {
            registerReceiver(receiver, filter, Context.RECEIVER_EXPORTED);
        } else {
            registerReceiver(receiver, filter);
        }
    }

    @Override
    public void onDestroy() {
        unregisterReceiver(receiver);
        super.onDestroy();
    }
}
