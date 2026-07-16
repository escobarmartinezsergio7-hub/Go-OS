#include <gtk/gtk.h>

static void on_activate(GtkApplication *app, gpointer user_data) {
    (void)user_data;
    GtkWidget *window = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(window), "ReduxOS Wayland GTK Demo");
    gtk_window_set_default_size(GTK_WINDOW(window), 640, 360);

    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 12);
    gtk_container_set_border_width(GTK_CONTAINER(box), 16);

    GtkWidget *title = gtk_label_new(NULL);
    gtk_label_set_markup(
        GTK_LABEL(title),
        "<span size=\"x-large\" weight=\"bold\">GTK3 on Wayland</span>"
    );
    gtk_label_set_xalign(GTK_LABEL(title), 0.0f);

    GtkWidget *status = gtk_label_new(
        "Si ves esta ventana, el cliente GTK se conecto a wayland-0."
    );
    gtk_label_set_line_wrap(GTK_LABEL(status), TRUE);
    gtk_label_set_xalign(GTK_LABEL(status), 0.0f);

    GtkWidget *hint = gtk_label_new(
        "Tip: mueve la ventana y prueba teclado/raton."
    );
    gtk_label_set_xalign(GTK_LABEL(hint), 0.0f);

    GtkWidget *button = gtk_button_new_with_label("Cerrar");
    g_signal_connect_swapped(button, "clicked", G_CALLBACK(gtk_window_close), window);

    gtk_box_pack_start(GTK_BOX(box), title, FALSE, FALSE, 0);
    gtk_box_pack_start(GTK_BOX(box), status, FALSE, FALSE, 0);
    gtk_box_pack_start(GTK_BOX(box), hint, FALSE, FALSE, 0);
    gtk_box_pack_end(GTK_BOX(box), button, FALSE, FALSE, 0);

    gtk_container_add(GTK_CONTAINER(window), box);
    gtk_widget_show_all(window);
}

int main(int argc, char **argv) {
    if (g_getenv("GDK_BACKEND") == NULL) {
        g_setenv("GDK_BACKEND", "wayland", TRUE);
    }
    GtkApplication *app = gtk_application_new(
        "com.reduxos.wayland.gtkdemo",
        G_APPLICATION_FLAGS_NONE
    );
    g_signal_connect(app, "activate", G_CALLBACK(on_activate), NULL);
    int status = g_application_run(G_APPLICATION(app), argc, argv);
    g_object_unref(app);
    return status;
}
