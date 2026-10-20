#include <gio/gio.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define BUS_NAME "org.freedesktop.DBus"
#define BUS_PATH "/org/freedesktop/DBus"
#define PLAYER_IFACE "org.mpris.MediaPlayer2.Player"
#define PROPERTIES_IFACE "org.freedesktop.DBus.Properties"
#define TIMEOUT_MS 1000

static void json_string(const gchar *input) {
    gchar *valid = g_utf8_make_valid(input ? input : "", -1);
    const gchar *p = valid;
    guint chars = 0;
    putchar('"');
    while (*p && chars < 512) {
        gunichar c = g_utf8_get_char(p);
        if (c == '"' || c == '\\') {
            putchar('\\');
            putchar((gchar)c);
        } else if (c < 0x20) {
            printf("\\u%04x", c);
        } else {
            gchar encoded[8] = {0};
            gint len = g_unichar_to_utf8(c, encoded);
            fwrite(encoded, 1, (size_t)len, stdout);
        }
        p = g_utf8_next_char(p);
        chars++;
    }
    putchar('"');
    g_free(valid);
}

static void emit(const gchar *artist, const gchar *track, const gchar *state) {
    printf("{\"version\":1,\"title\":\"Music\",\"rows\":[");
    printf("{\"label\":\"Artist\",\"value\":");
    json_string(artist);
    printf("},{\"label\":\"Track\",\"value\":");
    json_string(track);
    printf("},{\"label\":\"State\",\"value\":");
    json_string(state);
    puts("}]}");
    fflush(stdout);
}

static GVariant *call_sync(GDBusConnection *connection, const gchar *name,
                           const gchar *path, const gchar *interface,
                           const gchar *method, GVariant *parameters,
                           GError **error) {
    return g_dbus_connection_call_sync(connection, name, path, interface, method,
        parameters, NULL, G_DBUS_CALL_FLAGS_NONE, TIMEOUT_MS, NULL, error);
}

static int compare_cstrings(const void *left, const void *right) {
    const gchar *a = *(const gchar * const *)left;
    const gchar *b = *(const gchar * const *)right;
    return g_strcmp0(a, b);
}

static gchar *variant_string(GVariant *dict, const gchar *key, const gchar *fallback) {
    GVariant *value = g_variant_lookup_value(dict, key, NULL);
    gchar *result = NULL;
    if (value) {
        if (g_variant_is_of_type(value, G_VARIANT_TYPE_STRING))
            result = g_variant_dup_string(value, NULL);
        else if (g_variant_is_of_type(value, G_VARIANT_TYPE_VARIANT)) {
            GVariant *inner = g_variant_get_variant(value);
            if (g_variant_is_of_type(inner, G_VARIANT_TYPE_STRING))
                result = g_variant_dup_string(inner, NULL);
            g_variant_unref(inner);
        }
        g_variant_unref(value);
    }
    return result ? result : g_strdup(fallback);
}

static gchar *metadata_artist(GVariant *metadata) {
    GVariant *value = g_variant_lookup_value(metadata, "xesam:artist", NULL);
    gchar *result = NULL;
    if (value) {
        GVariant *actual = g_variant_is_of_type(value, G_VARIANT_TYPE_VARIANT)
            ? g_variant_get_variant(value) : g_variant_ref(value);
        if (g_variant_is_of_type(actual, G_VARIANT_TYPE_STRING))
            result = g_variant_dup_string(actual, NULL);
        else if (g_variant_is_of_type(actual, G_VARIANT_TYPE_STRING_ARRAY)) {
            GVariantIter iter;
            const gchar *item;
            g_variant_iter_init(&iter, actual);
            if (g_variant_iter_next(&iter, "&s", &item))
                result = g_strdup(item);
        }
        g_variant_unref(actual);
        g_variant_unref(value);
    }
    return result ? result : g_strdup("Unknown artist");
}

static gchar *metadata_title(GVariant *metadata) {
    GVariant *value = g_variant_lookup_value(metadata, "xesam:title", NULL);
    gchar *result = NULL;
    if (value) {
        GVariant *actual = g_variant_is_of_type(value, G_VARIANT_TYPE_VARIANT)
            ? g_variant_get_variant(value) : g_variant_ref(value);
        if (g_variant_is_of_type(actual, G_VARIANT_TYPE_STRING))
            result = g_variant_dup_string(actual, NULL);
        g_variant_unref(actual);
        g_variant_unref(value);
    }
    return result ? result : g_strdup("Unknown track");
}

static gchar *player_status(GDBusConnection *connection, const gchar *player) {
    GError *error = NULL;
    GVariant *reply = call_sync(connection, player, "/org/mpris/MediaPlayer2",
        PROPERTIES_IFACE, "Get",
        g_variant_new("(ss)", PLAYER_IFACE, "PlaybackStatus"), &error);
    if (!reply) {
        g_clear_error(&error);
        return NULL;
    }
    GVariant *boxed = g_variant_get_child_value(reply, 0);
    GVariant *value = g_variant_get_variant(boxed);
    gchar *status = g_variant_is_of_type(value, G_VARIANT_TYPE_STRING)
        ? g_variant_dup_string(value, NULL) : NULL;
    g_variant_unref(value);
    g_variant_unref(boxed);
    g_variant_unref(reply);
    return status;
}

static gchar *find_player(GDBusConnection *connection) {
    GError *error = NULL;
    GVariant *reply = call_sync(connection, BUS_NAME, BUS_PATH, BUS_NAME,
                                "ListNames", NULL, &error);
    if (!reply) {
        g_clear_error(&error);
        return NULL;
    }
    GVariant *names = g_variant_get_child_value(reply, 0);
    gsize count = 0;
    gchar **all_names = g_variant_dup_strv(names, &count);
    g_variant_unref(names);
    g_variant_unref(reply);
    qsort(all_names, count, sizeof(*all_names), compare_cstrings);
    gchar *fallback = NULL;
    guint checked = 0;
    for (gsize i = 0; i < count && checked < 4; i++) {
        if (!g_str_has_prefix(all_names[i], "org.mpris.MediaPlayer2."))
            continue;
        checked++;
        gchar *status = player_status(connection, all_names[i]);
        if (status && !fallback)
            fallback = g_strdup(all_names[i]);
        gboolean playing = status && g_str_equal(status, "Playing");
        g_free(status);
        if (playing) {
            g_free(fallback);
            fallback = g_strdup(all_names[i]);
            break;
        }
    }
    g_strfreev(all_names);
    return fallback;
}

static GDBusConnection *connect_session_bus(GError **error) {
    gchar *address = g_dbus_address_get_for_bus_sync(G_BUS_TYPE_SESSION, NULL, error);
    if (!address)
        return NULL;
    GDBusConnection *connection = g_dbus_connection_new_for_address_sync(
        address,
        G_DBUS_CONNECTION_FLAGS_AUTHENTICATION_CLIENT |
            G_DBUS_CONNECTION_FLAGS_MESSAGE_BUS_CONNECTION,
        NULL, NULL, error);
    g_free(address);
    if (connection)
        g_dbus_connection_set_exit_on_close(connection, FALSE);
    return connection;
}

static gboolean command_player(GDBusConnection *connection, const gchar *player,
                               const gchar *method) {
    GError *error = NULL;
    GVariant *reply = call_sync(connection, player, "/org/mpris/MediaPlayer2",
        PLAYER_IFACE, method, NULL, &error);
    if (!reply) {
        g_clear_error(&error);
        return FALSE;
    }
    g_variant_unref(reply);
    return TRUE;
}

int main(int argc, char **argv) {
    if (argc == 2) {
        const gchar *method = NULL;
        if (g_str_equal(argv[1], "--play-pause")) method = "PlayPause";
        else if (g_str_equal(argv[1], "--next")) method = "Next";
        else if (g_str_equal(argv[1], "--previous")) method = "Previous";
        else {
            fprintf(stderr, "usage: %s [--play-pause|--next|--previous]\n", argv[0]);
            return 2;
        }
        GError *error = NULL;
        GDBusConnection *connection = connect_session_bus(&error);
        if (!connection) {
            fprintf(stderr, "music: session bus unavailable: %s\n", error->message);
            g_clear_error(&error);
            return 1;
        }
        gchar *player = find_player(connection);
        gboolean ok = player && command_player(connection, player, method);
        g_free(player);
        g_object_unref(connection);
        return ok ? 0 : 1;
    }
    if (argc != 1) {
        fprintf(stderr, "usage: %s [--play-pause|--next|--previous]\n", argv[0]);
        return 2;
    }

    GDBusConnection *connection = NULL;
    for (;;) {
        if (!connection || g_dbus_connection_is_closed(connection)) {
            if (connection)
                g_object_unref(connection);
            GError *connect_error = NULL;
            connection = connect_session_bus(&connect_error);
            if (!connection) {
                emit("No session bus", "Unavailable", "Unavailable");
                g_clear_error(&connect_error);
                g_usleep(2 * G_USEC_PER_SEC);
                continue;
            }
        }
        gchar *player = find_player(connection);
        if (!player) {
            emit("No active player", "—", "Unavailable");
        } else {
            GError *call_error = NULL;
            GVariant *reply = call_sync(connection, player, "/org/mpris/MediaPlayer2",
                PROPERTIES_IFACE, "GetAll", g_variant_new("(s)", PLAYER_IFACE), &call_error);
            if (!reply) {
                emit("Unavailable", player, "Unavailable");
                g_clear_error(&call_error);
            } else {
                GVariant *props = g_variant_get_child_value(reply, 0);
                GVariant *metadata_value = g_variant_lookup_value(props, "Metadata", NULL);
                GVariant *metadata = metadata_value && g_variant_is_of_type(metadata_value, G_VARIANT_TYPE_VARIANT)
                    ? g_variant_get_variant(metadata_value)
                    : (metadata_value ? g_variant_ref(metadata_value) : NULL);
                gchar *status = variant_string(props, "PlaybackStatus", "Unknown");
                gchar *artist = metadata ? metadata_artist(metadata) : g_strdup("Unknown artist");
                gchar *title = metadata ? metadata_title(metadata) : g_strdup("Unknown track");
                emit(artist, title, status);
                g_free(status);
                g_free(artist);
                g_free(title);
                if (metadata) g_variant_unref(metadata);
                if (metadata_value) g_variant_unref(metadata_value);
                g_variant_unref(props);
                g_variant_unref(reply);
            }
        }
        g_free(player);
        g_usleep(2 * G_USEC_PER_SEC);
    }
}
