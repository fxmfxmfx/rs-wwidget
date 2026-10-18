/* Frozen v3 renderer: appearance baseline, independent of optimized drawing.
 * Uses the same deterministic input fixtures, but keeps its own palette and drawing. */
static const char *reference_months[] = {"ЯНВАРЬ","ФЕВРАЛЬ","МАРТ","АПРЕЛЬ","МАЙ","ИЮНЬ","ИЮЛЬ","АВГУСТ","СЕНТЯБРЬ","ОКТЯБРЬ","НОЯБРЬ","ДЕКАБРЬ"};
static const char *reference_weekdays[] = {"ВОСКРЕСЕНЬ","ПОНЕДЕЛЬНИК","ВТОРНИК","СРЕДА","ЧЕТВЕРГ","ПЯТНИЦА","СУББОТА"};

static const unsigned reference_FG=0xd4be98, reference_MUTED=0xa89984, reference_ACCENT=0xe78a4e, reference_GREEN=0xa9b665, reference_BORDER=0x504945;

static void reference_color(cairo_t *cr, unsigned hex, double alpha) {
    cairo_set_source_rgba(cr, ((hex>>16)&255)/255., ((hex>>8)&255)/255., (hex&255)/255., alpha);
}
static void reference_box(cairo_t *cr, double x, double y, double w, double h, unsigned hex, double alpha) {
    reference_color(cr,hex,alpha); cairo_rectangle(cr,x,y,w,h); cairo_fill(cr);
}
static void reference_text(cairo_t *cr, const char *s, double x, double y, int size, unsigned hex, int width, PangoAlignment align) {
    PangoLayout *layout=pango_cairo_create_layout(cr);
    PangoFontDescription *font=pango_font_description_new();
    pango_font_description_set_family(font,"MxPlus IBM VGA 8x16");
    pango_font_description_set_absolute_size(font,size*PANGO_SCALE);
    pango_layout_set_font_description(layout,font);
    pango_layout_set_text(layout,s,-1);
    if (width>0) { pango_layout_set_width(layout,width*PANGO_SCALE); pango_layout_set_alignment(layout,align); }
    reference_color(cr,hex,1); cairo_move_to(cr,x,y); pango_cairo_show_layout(cr,layout);
    pango_font_description_free(font); g_object_unref(layout);
}
static void reference_card(cairo_t *cr, int y, int h) {
    reference_box(cr,0,y,WIDTH,h,0x282828,.96);
    reference_color(cr,reference_BORDER,1); cairo_set_line_width(cr,1); cairo_rectangle(cr,.5,y+.5,WIDTH-1,h-1); cairo_stroke(cr);
    reference_box(cr,0,y+18,2,16,reference_ACCENT,1);
    for(int row=0;row<3;row++) reference_box(cr,268,y+22+row*4,10,1,reference_MUTED,.7);
}
static void reference_bar(cairo_t *cr, int y, double fraction, unsigned hex) {
    reference_box(cr,24,y,252,3,0x3c3836,1);
    reference_box(cr,24,y,252*CLAMP(fraction,0.,1.),3,hex,1);
}

static void reference_render_panel(cairo_t *cr,int index) {
    static const int offsets[]={0,160,450};
    char s[128];
    cairo_save(cr); cairo_translate(cr,0,-offsets[index]);
    reference_card(cr,offsets[index],panel_heights[index]);
    if(index==0) {
        reference_text(cr,"СЕЙЧАС",24,20,16,reference_MUTED,0,PANGO_ALIGN_LEFT);
        strftime(s,sizeof s,"%H:%M",&now);
        reference_text(cr,s,24,42,64,reference_FG,252,PANGO_ALIGN_CENTER);
        snprintf(s,sizeof s,"%s · %02d.%02d",reference_weekdays[now.tm_wday],now.tm_mday,now.tm_mon+1);
        reference_text(cr,s,24,113,16,reference_ACCENT,252,PANGO_ALIGN_CENTER);
    } else if(index==1) {
        reference_text(cr,"КАЛЕНДАРЬ",24,180,16,reference_MUTED,0,PANGO_ALIGN_LEFT);
        snprintf(s,sizeof s,"%s %d",reference_months[now.tm_mon],now.tm_year+1900);
        reference_text(cr,s,24,208,16,reference_FG,252,PANGO_ALIGN_CENTER);
        static const char *days[]={"ПН","ВТ","СР","ЧТ","ПТ","СБ","ВС"};
        for (int c=0;c<7;c++) reference_text(cr,days[c],24+c*36,244,16,c>=5?reference_ACCENT:reference_MUTED,36,PANGO_ALIGN_CENTER);
        int first=month_start(now.tm_year+1900,now.tm_mon+1);
        for (int day=1;day<=month_days(now.tm_year+1900,now.tm_mon+1);day++) {
            int cell=first+day-1, col=cell%7, row=cell/7, x=24+col*36, y=272+row*26;
            bool today=day==now.tm_mday;
            if (today) reference_box(cr,x+4,y-3,28,22,reference_ACCENT,1);
            snprintf(s,sizeof s,"%2d",day);
            reference_text(cr,s,x,y,16,today?0x282828:(col>=5?reference_ACCENT:reference_FG),36,PANGO_ALIGN_CENTER);
        }
    } else {
        reference_text(cr,"СИСТЕМА",24,470,16,reference_MUTED,0,PANGO_ALIGN_LEFT);
        reference_text(cr,"CPU",24,500,16,reference_FG,0,PANGO_ALIGN_LEFT);
        if (cpu_ready) snprintf(s,sizeof s,"%.0f%%",cpu); else g_strlcpy(s,"—",sizeof s);
        reference_text(cr,s,72,500,16,reference_ACCENT,204,PANGO_ALIGN_RIGHT); reference_bar(cr,524,cpu_ready?cpu/100.:0,reference_ACCENT);
        reference_text(cr,"ОЗУ",24,540,16,reference_FG,0,PANGO_ALIGN_LEFT);
        if (memory_ready) snprintf(s,sizeof s,"%.1f / %.1f GiB",ram_used,ram_total); else g_strlcpy(s,"—",sizeof s);
        reference_text(cr,s,72,540,16,reference_FG,204,PANGO_ALIGN_RIGHT); reference_bar(cr,564,memory_ready?ram_used/ram_total:0,reference_GREEN);
        reference_text(cr,"ДИСК",24,580,16,reference_FG,0,PANGO_ALIGN_LEFT);
        if (disk_ready) snprintf(s,sizeof s,"%.0f GiB своб.",disk_free); else g_strlcpy(s,"—",sizeof s);
        reference_text(cr,s,72,580,16,reference_FG,204,PANGO_ALIGN_RIGHT); reference_bar(cr,604,disk_ready?disk_used_fraction:0,reference_MUTED);
    }
    cairo_restore(cr);
}

static void reference_render_grid(cairo_t *cr,int width,int height) {
    reference_box(cr,0,0,width,height,0x1b1b1b,.08);
    cairo_set_line_width(cr,1);
    for(int major=0;major<2;major++) {
        reference_color(cr,reference_ACCENT,major?.32:.13);
        for(int x=0;x<width;x+=GRID_SIZE) {
            if((x%(GRID_SIZE*4)==0)!=major) continue;
            cairo_move_to(cr,x+.5,0); cairo_line_to(cr,x+.5,height);
        }
        for(int y=0;y<height;y+=GRID_SIZE) {
            if((y%(GRID_SIZE*4)==0)!=major) continue;
            cairo_move_to(cr,0,y+.5); cairo_line_to(cr,width,y+.5);
        }
        cairo_stroke(cr);
    }
}
