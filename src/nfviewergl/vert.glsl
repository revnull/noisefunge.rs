in vec4 position;
in vec4 color;

out vec4 v_color;

void main() {
    v_color = color;

    gl_Position = position;
}
