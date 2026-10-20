#include <gio/gio.h>
#include <stdio.h>
#include <string.h>

#define PLAYER_IFACE "org.mpris.MediaPlayer2.Player"
#define OBJECT_PATH "/org/mpris/MediaPlayer2"
#define PROPERTIES_IFACE "org.freedesktop.DBus.Properties"

static const gchar *introspection_xml =
    "<node><interface name='org.mpris.MediaPlayer2.Player'>"
    "<method name='PlayPause'/>"
    "<method name='Next'/>"
    "<method name='Previous'/>"
    "<property name='PlaybackStatus' type='s' access='read'/>"
    "<property name='Metadata' type='a{sv}' access='read'/>"
    "</interface></node>";

typedef struct {
    gchar *status;
} Player;

static void method_call(GDBusConnection *connection, const gchar *sender,
                        const gchar *object_path, const gchar *interface_name,
                        const gchar *method_name, GVariant *parameters,
                        GDBusMethodInvocation *invocation, gpointer data) {
    (void)connection; (void)sender; (void)object_path; (void)interface_name; (void)parameters;
    Player *player = data;
    if (g_str_equal(method_name, "PlayPause")) {
        gboolean was_playing = g_str_equal(player->status, "Playing");
        g_free(player->status);
        player->status = g_strdup(was_playing ? "Paused" : "Playing");
    }
    g_dbus_method_invocation_return_value(invocation, g_variant_new("()"));
}

static GVariant *get_property(GDBusConnection *connection, const gchar *sender,
                              const gchar *object_path, const gchar *interface_name,
                              const gchar *property_name, GError **error,
                              gpointer data) {
    (void)connection; (void)sender; (void)object_path; (void)interface_name; (void)error;
    Player *player = data;
    if (g_str_equal(property_name, "PlaybackStatus"))
        return g_variant_new_string(player->status);
    if (g_str_equal(property_name, "Metadata")) {
        GVariantBuilder builder;
        g_variant_builder_init(&builder, G_VARIANT_TYPE("a{sv}"));
        const gchar *artists[] = {"Artist \"雪", NULL};
        g_variant_builder_add(&builder, "{sv}", "xesam:artist", g_variant_new_strv(artists, -1));
        g_variant_builder_add(&builder, "{sv}", "xesam:title", g_variant_new_string("Track \\ \"𝄞"));
        return g_variant_builder_end(&builder);
    }
    return NULL;
}

static const GDBusInterfaceVTable vtable = {
    .method_call = method_call,
    .get_property = get_property,
    .set_property = NULL,
};

static GDBusConnection *connect_bus(GError **error) {
    GDBusConnection *connection = g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, error);
    if (connection)
        g_dbus_connection_set_exit_on_close(connection, FALSE);
    return connection;
}

static int serve(const gchar *name, const gchar *initial_status) {
    GError *error = NULL;
    GDBusConnection *connection = connect_bus(&error);
    if (!connection) {
        fprintf(stderr, "%s\n", error->message);
        g_clear_error(&error);
        return 1;
    }
    GVariant *reply = g_dbus_connection_call_sync(connection,
        "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
        "RequestName", g_variant_new("(su)", name, 0u), G_VARIANT_TYPE("(u)"),
        G_DBUS_CALL_FLAGS_NONE, 1000, NULL, &error);
    if (!reply) {
        fprintf(stderr, "%s\n", error->message);
        g_clear_error(&error);
        g_object_unref(connection);
        return 1;
    }
    guint32 result = 0;
    g_variant_get(reply, "(u)", &result);
    g_variant_unref(reply);
    if (result != 1 && result != 4) {
        g_object_unref(connection);
        return 1;
    }

    Player player = {.status = g_strdup(initial_status)};
    GDBusNodeInfo *node = g_dbus_node_info_new_for_xml(introspection_xml, &error);
    if (!node) {
        fprintf(stderr, "%s\n", error->message);
        g_clear_error(&error);
        g_free(player.status);
        g_object_unref(connection);
        return 1;
    }
    guint registration = g_dbus_connection_register_object(connection, OBJECT_PATH,
        node->interfaces[0], &vtable, &player, NULL, &error);
    if (!registration) {
        fprintf(stderr, "%s\n", error->message);
        g_clear_error(&error);
        g_dbus_node_info_unref(node);
        g_free(player.status);
        g_object_unref(connection);
        return 1;
    }
    GMainLoop *loop = g_main_loop_new(NULL, FALSE);
    g_main_loop_run(loop);
    g_main_loop_unref(loop);
    g_dbus_connection_unregister_object(connection, registration);
    g_dbus_node_info_unref(node);
    g_free(player.status);
    g_object_unref(connection);
    return 0;
}

static int status(const gchar *name) {
    GError *error = NULL;
    GDBusConnection *connection = connect_bus(&error);
    if (!connection) {
        g_clear_error(&error);
        return 1;
    }
    GVariant *reply = g_dbus_connection_call_sync(connection, name, OBJECT_PATH,
        PROPERTIES_IFACE, "Get", g_variant_new("(ss)", PLAYER_IFACE, "PlaybackStatus"),
        G_VARIANT_TYPE("(v)"), G_DBUS_CALL_FLAGS_NONE, 1000, NULL, &error);
    if (!reply) {
        g_clear_error(&error);
        g_object_unref(connection);
        return 1;
    }
    GVariant *boxed = g_variant_get_child_value(reply, 0);
    GVariant *value = g_variant_get_variant(boxed);
    puts(g_variant_get_string(value, NULL));
    g_variant_unref(value);
    g_variant_unref(boxed);
    g_variant_unref(reply);
    g_object_unref(connection);
    return 0;
}

int main(int argc, char **argv) {
    if (argc == 4 && g_str_equal(argv[1], "--serve"))
        return serve(argv[2], argv[3]);
    if (argc == 3 && g_str_equal(argv[1], "--status"))
        return status(argv[2]);
    fprintf(stderr, "usage: %s --serve BUS_NAME Playing|Paused | --status BUS_NAME\n", argv[0]);
    return 2;
}
